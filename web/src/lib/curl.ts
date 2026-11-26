// Parses the commands the REPL accepts: a small subset of curl, or a bare
// "METHOD URL [body]" line. Anything outside the subset is an error that
// names the unsupported part, never a silent guess.

export interface ParsedRequest {
  node: number | null; // from 0; null means the load balancer
  method: string;
  path: string;
  headers: [string, string][];
  body: string;
  include: boolean; // show response headers (-i)
}

export type ParseResult = { ok: true; request: ParsedRequest } | { ok: false; error: string };

export const HTTP_PORT = 8098;

/** Split like a POSIX shell would for the cases people paste: quotes and backslashes. */
export function tokenize(input: string): string[] | string {
  const tokens: string[] = [];
  let cur = "";
  let inToken = false;
  let i = 0;
  const s = input.replace(/\\\r?\n/g, " ");
  while (i < s.length) {
    const c = s[i];
    if (c === "'") {
      const end = s.indexOf("'", i + 1);
      if (end < 0) return "A single quote is not closed.";
      cur += s.slice(i + 1, end);
      inToken = true;
      i = end + 1;
    } else if (c === '"') {
      i++;
      while (i < s.length && s[i] !== '"') {
        if (s[i] === "\\" && i + 1 < s.length && '"\\$`'.includes(s[i + 1])) i++;
        cur += s[i++];
      }
      if (i >= s.length) return "A double quote is not closed.";
      inToken = true;
      i++;
    } else if (c === "\\" && i + 1 < s.length) {
      cur += s[i + 1];
      inToken = true;
      i += 2;
    } else if (/\s/.test(c)) {
      if (inToken) tokens.push(cur);
      cur = "";
      inToken = false;
      i++;
    } else {
      cur += c;
      inToken = true;
      i++;
    }
  }
  if (inToken) tokens.push(cur);
  return tokens;
}

/** Where a URL's host sends the request: localhost (or lb) is the load
 *  balancer, nodeN is node N directly. */
export function parseUrl(url: string, nodes: number): { node: number | null; path: string } | string {
  const m = /^(?:https?:\/\/)?([^/:?]+)(?::(\d+))?(\/[^\s]*)?$/.exec(url);
  if (!m) return `The URL "${url}" is not valid.`;
  const [, host, port, path] = m;
  if (url.startsWith("https://")) return "The simulator uses HTTP, not HTTPS.";
  if (port !== undefined && Number(port) !== HTTP_PORT) return `The HTTP port is ${HTTP_PORT}, not ${port}.`;
  if (host === "localhost" || host === "127.0.0.1" || host === "lb") return { node: null, path: path ?? "/" };
  const n = /^node(\d+)$/.exec(host);
  if (!n) return `Unknown host "${host}". Use localhost for the load balancer, or node1 to node${nodes}.`;
  const node = Number(n[1]) - 1;
  if (node < 0 || node >= nodes) return `There is no ${host} in a cluster of ${nodes} nodes.`;
  return { node, path: path ?? "/" };
}

const METHODS = ["GET", "PUT", "POST", "DELETE", "HEAD"];

export function parseCommand(input: string, nodes: number): ParseResult {
  const tokens = tokenize(input.trim());
  if (typeof tokens === "string") return { ok: false, error: tokens };
  if (tokens.length === 0) return { ok: false, error: "Type a command." };

  if (METHODS.includes(tokens[0].toUpperCase())) {
    if (tokens.length < 2) return { ok: false, error: `Add a URL or a path after ${tokens[0]}.` };
    const target = tokens[1].startsWith("/") ? { node: null, path: tokens[1] } : parseUrl(tokens[1], nodes);
    if (typeof target === "string") return { ok: false, error: target };
    const body = tokens.slice(2).join(" ");
    return {
      ok: true,
      request: { ...target, method: tokens[0].toUpperCase(), headers: body ? [["Content-Type", "text/plain"]] : [], body, include: true },
    };
  }

  if (tokens[0] !== "curl") return { ok: false, error: `"${tokens[0]}" is not a command. Start the command with curl or an HTTP method.` };

  let method: string | undefined;
  let url: string | undefined;
  const headers: [string, string][] = [];
  let body: string | undefined;
  let include = false;
  for (let i = 1; i < tokens.length; i++) {
    const t = tokens[i];
    const value = () => {
      const v = tokens[++i];
      if (v === undefined) throw new Error(`Add a value after ${t}.`);
      return v;
    };
    try {
      if (t === "-X" || t === "--request") method = value().toUpperCase();
      else if (t === "-H" || t === "--header") {
        const h = value();
        const colon = h.indexOf(":");
        if (colon < 1) return { ok: false, error: `The header "${h}" has no name.` };
        headers.push([h.slice(0, colon).trim(), h.slice(colon + 1).trim()]);
      } else if (["-d", "--data", "--data-raw", "--data-binary"].includes(t)) {
        const v = value();
        if (v.startsWith("@") && t !== "--data-raw") return { ok: false, error: "The simulator cannot read a body from a file (@file)." };
        body = body === undefined ? v : `${body}&${v}`;
      } else if (t === "-i" || t === "--include") include = true;
      else if (t === "-s" || t === "--silent" || t === "-S" || t === "--show-error") continue;
      else if (/^-[isS]{2,}$/.test(t)) include ||= t.includes("i");
      else if (t.startsWith("-")) return { ok: false, error: `The curl option ${t} is not supported.` };
      else if (url === undefined) url = t;
      else return { ok: false, error: `Unexpected "${t}". A command can have one URL only.` };
    } catch (e) {
      return { ok: false, error: (e as Error).message };
    }
  }
  if (url === undefined) return { ok: false, error: "Add a URL to the curl command." };
  const target = parseUrl(url, nodes);
  if (typeof target === "string") return { ok: false, error: target };
  method ??= body !== undefined ? "POST" : "GET";
  if (!METHODS.includes(method)) return { ok: false, error: `The method ${method} is not supported.` };
  if (body !== undefined && !headers.some(([k]) => k.toLowerCase() === "content-type")) {
    headers.push(["Content-Type", "application/x-www-form-urlencoded"]); // what curl sends with -d
  }
  return { ok: true, request: { ...target, method, headers, body: body ?? "", include } };
}
