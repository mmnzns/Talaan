import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function App() {
  const [platform, setPlatform] = useState("");

  useEffect(() => {
    invoke<string>("platform_name").then(setPlatform).catch(() => setPlatform("browser"));
  }, []);

  return (
    <main className="flex h-full flex-col items-center justify-center gap-3">
      <h1 className="text-4xl font-bold tracking-tight">Talaan</h1>
      <p className="font-mono text-sm text-subtle">1.0.0-dev · {platform}</p>
    </main>
  );
}
