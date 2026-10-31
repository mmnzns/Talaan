# Summary languages with the local AI

Which of the 32 summary languages each local AI model writes poorly. The languages graded weak or bad are the ones that
show "Works best with online AI" when that model is the summary model (`POOR_ON_LOCAL` in
`src/lib/summaryLanguages.ts`, one list per model id).

| Model | Measured | Languages with the note |
| --- | --- | --- |
| `qwen3.5-4b` | yes | 17 of 32 (see below) |
| `granite-4.2-3b` | yes | 30 of 32: everything but English and Chinese (Simplified) |
| `qwen3.5-9b` | no | none until measured: it is not on the test Mac and is a 5.5 GB download |

## Method

- Models: Q4_K_M files, Metal (card on), thinking off, as the app runs them. Same method for every model.
- Input: `fixtures/summary.language-check.json`, a synthetic English meeting (about 10 minutes, 3 speakers, decisions,
  action items with owners and dates, one open question, and one name slip: the speaker says "Dana" and is corrected to
  "Elena").
- One summary per language through the real path (`summarize()`: Talaan's prompt, schema, retry and parser), General
  meeting type, no custom instructions. Run: `language_check` in `src-tauri/src/summarizer/local.rs` (ignored test).
- Graded by reading each result: parses as a Talaan summary; in the target language and script throughout; facts kept
  (owners, dates, decisions); fluent enough to send to a colleague.
  - good: sendable, at most small slips.
  - weak: a native speaker would flag it: invented or wrong words, wrong meaning, wrong dates, mixed languages.
  - bad: unusable: degenerate text or facts lost.
- One run per language, temperature 0.2, so a language near a boundary could grade differently on another run.

## Qwen 3.5 4B

### Results

All 32 replies parsed as Talaan summaries on the first try (no retries). The reply is JSON the model is forced to follow,
so a bad language still parses; the problems are in the words. Total 1392 s for 32 summaries, median
42 s each (range 32 to 61 s).

| Language | Grade | Reason | Parse ok | Seconds |
| --- | --- | --- | --- | --- |
| English (`en`) | good | Clean and accurate; every owner and date right. | yes | 61 |
| Filipino (`fil`) | weak | Heavy English mixing ("Monday, October 27", "action items"); "plansa" (an iron) for "plans"; one decision turns garbled ("Hindi babalik ang public announcement hangga hindi na siya confirmed"). | yes | 51 |
| Spanish (`es`) | good | Fluent and accurate; one garbled word ("permitircnos"). | yes | 44 |
| French (`fr`) | good | Fluent and accurate; recap says the launch was delayed, which it was not. | yes | 50 |
| German (`de`) | good | Fluent; "Last-Test" for Lasttest and "an den User" are slips; due dates left as English weekdays. | yes | 44 |
| Portuguese (`pt`) | good | Fluent and accurate; "Me vai escrever" treats the speaker label as a name. | yes | 39 |
| Italian (`it`) | good | Fluent; owner name misspelled once ("Raffael"); "Me (utente)". | yes | 39 |
| Dutch (`nl`) | good | Fluent; only 5 of 8 action items (merges the Chen question into the FAQ item); "draagt de load test uit" is odd. | yes | 32 |
| Swedish (`sv`) | weak | Opens with "Kallan handlade om" (nonsense); wrong gender and word order ("inget offentligt sociala meddelande", "Team-planet"). | yes | 39 |
| Danish (`da`) | weak | Garbled words ("Mødnet", "prisskriftet"); due dates left in English. | yes | 41 |
| Norwegian (`no`) | weak | Mixes Nynorsk and Bokmål ("handla", "Innsender iOS-build", "Kjør ... og deler"); wrong verb forms in action items. | yes | 39 |
| Finnish (`fi`) | weak | Broken inflection and sense in the recap ("Hinnataukosta", "Team-tarjous palautetaan tammikuuhun"); due date "fridaatti" is not a word. | yes | 41 |
| Polish (`pl`) | weak | A Russian word inside a decision ("publicznej анонсы beta") breaks the script; otherwise decent Polish. | yes | 41 |
| Czech (`cs`) | good | Mostly fluent; a few grammar slips ("tým Team", "bude zobrazeno pouze dva plány"); "Me (uživatel)". | yes | 44 |
| Romanian (`ro`) | weak | Invented words ("trimiteră"), wrong weekdays (Monday 27 given as marți), garbled action item; dues in English. | yes | 45 |
| Greek (`el`) | weak | Invented words ("διανοήσης", "Διαβύθιση", "Γράφηση"), Monday 27 given as Friday, English words left in ("waitlist", "sync"). | yes | 59 |
| Russian (`ru`) | good | Fluent and accurate; due dates left as English weekdays. | yes | 43 |
| Ukrainian (`uk`) | good | Fluent; small gender and preposition slips ("тестове збірку", "зі синхронізацією"). | yes | 53 |
| Turkish (`tr`) | weak | A decision reads backwards ("açılacak, ancak ... onaylanana kadar") and one gives an invented reason (server load) for holding the announcement. | yes | 46 |
| Arabic (`ar`) | weak | "Plans" came out as "airplanes" (طائرتين) in the recap and a decision; "Android" became "mobile"; QA condition garbled; dues in English. | yes | 40 |
| Hebrew (`he`) | bad | Wrong weekdays on most dates, garbled names ("מאני" for Me, "Ap Harbor"), the database became a "network" (רשת). | yes | 51 |
| Hindi (`hi`) | weak | "QA" read as the word "क्वा" in a decision; the user named "मेरी"; due dates left as English. | yes | 47 |
| Bengali (`bn`) | weak | Nonsense for "sign off" ("স্যাফি-ফাই"), "budget" misspelled, Monday 27 given as Tuesday, one action item says "to learn about" instead of "ask"; dues in English. | yes | 51 |
| Urdu (`ur`) | bad | Degenerate: the same invented phrase ("اندرونی ٹیکنیکل ٹیم") fills the recap, every decision and every action item; facts lost; "Team plan returns in October". | yes | 55 |
| Indonesian (`id`) | good | Fluent and accurate; dues left as English weekdays. | yes | 36 |
| Malay (`ms`) | weak | "Plans" and "build" both rendered as "senaraian" (list) ("Menghantar senaraian iOS"); "Mencadangkan Chen" wrong. | yes | 37 |
| Vietnamese (`vi`) | weak | Fluent, but weekdays wrong on most dated items (Oct 17 given as Thursday); "bản xây dựng iOS" is a literal mistranslation of "build". | yes | 47 |
| Thai (`th`) | good | Fluent; due dates left as English; the open question is a fragment. | yes | 34 |
| Japanese (`ja`) | good | Fluent and accurate; "Android QA 承認付き" is stiff; load test tied to launch day. | yes | 38 |
| Korean (`ko`) | weak | Every date is spaced ("10 월 27 일"); app name transliterated oddly ("해버"); only 7 action items. | yes | 37 |
| Chinese (Simplified) (`zh-Hans`) | good | Fluent and accurate in Simplified; merges some items. | yes | 35 |
| Chinese (Traditional) (`zh-Hant`) | good | Fluent in Traditional characters; "候選名單" is an odd word for waitlist; some items merged. | yes | 37 |

### What shows the note

Qwen 3.5 4B: 17 of 32 (weak: fil, sv, da, no, fi, pl, ro, el, tr, ar, hi, bn, ms, vi, ko; bad: he, ur).
Good: en, es, fr, de, pt, it, nl, cs, ru, uk, id, th, ja, zh-Hans, zh-Hant.


### Findings worth knowing

- Due dates are not written in the target language in several languages that are otherwise fine (de, ru, id, th, hi,
  bn, ar, ro, da and others show "Friday, October 17"). The prompt tells the model to give the deadline "as said" ("Friday", "Oct 17"), so it copies the
  transcript's words. A prompt line for due dates would fix it; not changed here.
- The user's own speaker label "Me" is treated as a name in many languages ("Me vai escrever", "Me (utente)").
- Several languages invent weekdays for a date (vi, ro, el, he, bn).

## Granite 4.2 3B

Same fixture, prompt and bar. All 32 replies parsed on the first try. Total 835 s for 32 summaries, median
26 s each (range 21 to 34 s), faster than Qwen 3.5 4B.

The headline: Granite mostly ignores "Write everything in <language>". In 29 of the 31 non-English languages it wrote the whole summary
in English (English words and Latin script throughout, including for Russian, Arabic, Japanese and the other non-Latin
languages). Only German and Chinese (Simplified) came back in the language asked for. So for Granite the note should
show on almost every language: for those, a summary in the wrong language is the likely result, which is the first
thing the spec says to flag.

| Language | Grade | Reason | Parse ok | Seconds |
| --- | --- | --- | --- | --- |
| English (`en`) | good | Accurate owners and dates; decisions repeat the action items, and "Me" is treated as a name. | yes | 28 |
| Filipino (`fil`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 23 |
| Spanish (`es`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 28 |
| French (`fr`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 27 |
| German (`de`) | weak | German, but with wrong verb forms ("Me ... prüfe", "teile"), a made-up compound ("Beta-Veröffentlichungdatum"), an English comment in an owner ("Chen (implizit, aber nicht im Transkript genannt)"), and English due dates. | yes | 31 |
| Portuguese (`pt`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 25 |
| Italian (`it`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 25 |
| Dutch (`nl`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 28 |
| Swedish (`sv`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 27 |
| Danish (`da`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 26 |
| Norwegian (`no`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 26 |
| Finnish (`fi`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 25 |
| Polish (`pl`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 34 |
| Czech (`cs`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 24 |
| Romanian (`ro`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 24 |
| Greek (`el`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 27 |
| Russian (`ru`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 21 |
| Ukrainian (`uk`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 25 |
| Turkish (`tr`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 26 |
| Arabic (`ar`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 23 |
| Hebrew (`he`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 25 |
| Hindi (`hi`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 29 |
| Bengali (`bn`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 27 |
| Urdu (`ur`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 27 |
| Indonesian (`id`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 24 |
| Malay (`ms`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 26 |
| Vietnamese (`vi`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 23 |
| Thai (`th`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 24 |
| Japanese (`ja`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 26 |
| Korean (`ko`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 27 |
| Chinese (Simplified) (`zh-Hans`) | good | Real Simplified Chinese with Latin plan names; "Me" kept as "我（Me）"; Chen item given the wrong due date; due fields in English. | yes | 32 |
| Chinese (Traditional) (`zh-Hant`) | bad | Wrote the whole summary in English, ignoring the language. | yes | 23 |

Granite shows the note for 30 of 32 languages (bad: all but en, de, zh-Hans; weak: de).

## Qwen 3.5 9B

Not measured. It is not on the test Mac and needs a 5.5 GB download, so its list is empty and it shows no note until
it is run (`TALAAN_LLM_MODEL=<gguf> TALAAN_LANGCHECK_OUT=<folder> cargo test --release language_check -- --ignored --nocapture`).
