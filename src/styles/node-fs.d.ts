// The one Node API a test uses (tokens.test.ts reads tokens.css); the app has no Node types.
declare module "node:fs" {
  export function readFileSync(path: string | URL, encoding: "utf8"): string;
}
