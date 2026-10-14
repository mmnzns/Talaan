//! talaan-llm: Talaan's local AI helper. Runs llama.cpp in its own process so its copy of ggml never meets
//! Whisper's (see docs/plans/M5-summarizer.md), and so a model crash or out-of-memory never takes the app down.
//!
//! Reads one JSON request per line on stdin, writes JSON events per line on stdout (protocol crate). Exits when
//! stdin closes or after a while without requests. Logs go to stderr.

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use std::io::{BufRead, Write};
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use talaan_llm_protocol::{encode, Completion, Event, Phase, Reply, Request};

const IDLE_EXIT: Duration = Duration::from_secs(120);
const BATCH: usize = 512;

fn send(ev: &Event) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", encode(ev));
    let _ = out.flush();
}

fn main() {
    // Quiet llama.cpp's own logging; errors still come back as events.
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(false));
    let backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => return send(&Event::Error { message: format!("couldn't start the local AI: {e}") }),
    };

    // stdin on its own thread so the main loop can time out when idle.
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut loaded: Option<(String, LlamaModel)> = None;
    loop {
        let line = match rx.recv_timeout(IDLE_EXIT) {
            Ok(l) => l,
            Err(_) => break, // idle or stdin closed: free everything by exiting
        };
        match serde_json::from_str::<Request>(&line) {
            Ok(Request::Ping) => send(&Event::Ready),
            Ok(Request::Complete(c)) => match complete(&backend, &mut loaded, &c) {
                Ok(reply) => send(&Event::Done(reply)),
                Err(message) => send(&Event::Error { message }),
            },
            Err(e) => send(&Event::Error { message: format!("bad request: {e}") }),
        }
    }
}

fn complete(backend: &LlamaBackend, loaded: &mut Option<(String, LlamaModel)>, c: &Completion) -> Result<Reply, String> {
    let started = Instant::now();
    if loaded.as_ref().map(|(p, _)| p != &c.model_path).unwrap_or(true) {
        *loaded = None;
        send(&Event::Progress { phase: Phase::Loading, done: 0, total: 0 });
        let params = LlamaModelParams::default().with_n_gpu_layers(999);
        let model = LlamaModel::load_from_file(backend, &c.model_path, &params).map_err(|e| format!("couldn't load the AI model: {e}"))?;
        *loaded = Some((c.model_path.clone(), model));
    }
    let model = &loaded.as_ref().expect("loaded above").1;

    let template = model.chat_template(None).map_err(|e| format!("the model has no chat template: {e}"))?;
    let messages = [
        LlamaChatMessage::new("system".into(), c.system.clone()).map_err(|e| e.to_string())?,
        LlamaChatMessage::new("user".into(), c.user.clone()).map_err(|e| e.to_string())?,
    ];
    let prompt = model.apply_chat_template(&template, &messages, true).map_err(|e| format!("couldn't build the prompt: {e}"))?;
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
    let n_prompt = tokens.len() as u32;
    if n_prompt + c.max_tokens > c.n_ctx {
        return Err(format!("the transcript part is too long for the model ({n_prompt} tokens, context {})", c.n_ctx));
    }

    let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4).clamp(2, 8);
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(c.n_ctx))
        .with_n_batch(BATCH as u32)
        .with_n_ubatch(BATCH as u32)
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    let mut ctx = model.new_context(backend, ctx_params).map_err(|e| format!("not enough memory for the AI model: {e}"))?;

    // Read the prompt in batches; only the last token needs logits.
    let mut batch = LlamaBatch::new(BATCH, 1);
    for (start, chunk) in tokens.chunks(BATCH).enumerate().map(|(i, ch)| (i * BATCH, ch)) {
        batch.clear();
        for (j, t) in chunk.iter().enumerate() {
            let pos = start + j;
            batch.add(*t, pos as i32, &[0], pos + 1 == tokens.len()).map_err(|e| e.to_string())?;
        }
        ctx.decode(&mut batch).map_err(|e| format!("the AI model failed reading: {e}"))?;
        send(&Event::Progress { phase: Phase::Reading, done: (start + chunk.len()) as u32, total: n_prompt });
    }

    let mut samplers = vec![];
    if let Some(schema) = &c.json_schema {
        let grammar = llama_cpp_2::json_schema_to_grammar(schema).map_err(|e| format!("bad output schema: {e}"))?;
        samplers.push(LlamaSampler::grammar(model, &grammar, "root").map_err(|e| format!("bad output schema: {e}"))?);
    }
    if c.temperature <= 0.0 {
        samplers.push(LlamaSampler::greedy());
    } else {
        samplers.push(LlamaSampler::top_k(40));
        samplers.push(LlamaSampler::top_p(0.9, 1));
        samplers.push(LlamaSampler::temp(c.temperature));
        samplers.push(LlamaSampler::dist(1234));
    }
    let mut sampler = LlamaSampler::chain_simple(samplers);

    let mut out: Vec<u8> = vec![];
    let mut n_cur = tokens.len() as i32;
    let mut written = 0u32;
    let mut truncated = true;
    while written < c.max_tokens {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if vocab.is_eog(token) {
            truncated = false;
            break;
        }
        out.extend(vocab.token_to_piece(token, false, None));
        written += 1;
        if written % 32 == 0 {
            send(&Event::Progress { phase: Phase::Writing, done: written, total: 0 });
        }
        batch.clear();
        batch.add(token, n_cur, &[0], true).map_err(|e| e.to_string())?;
        n_cur += 1;
        ctx.decode(&mut batch).map_err(|e| format!("the AI model failed writing: {e}"))?;
    }

    Ok(Reply {
        text: String::from_utf8_lossy(&out).into_owned(),
        prompt_tokens: n_prompt,
        output_tokens: written,
        seconds: started.elapsed().as_secs_f32(),
        truncated,
    })
}
