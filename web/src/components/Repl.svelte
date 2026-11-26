<script lang="ts">
  // The console: a command line for curl requests and riak commands.
  // Replies arrive later, in simulation time, and are matched to their
  // command by request id.
  import Button from "./Button.svelte";
  import { parseCommand } from "../lib/curl.ts";
  import type { Cluster, HttpResponse } from "../lib/sim.ts";

  let {
    cluster,
    nodes,
    onsubmit,
    onpower,
    onadmin = () => ({ ok: false, text: "Cluster commands are not available here." }),
    open = $bindable(false),
  }: {
    cluster: Cluster;
    nodes: number;
    onsubmit: (id: number, request: import("../lib/curl.ts").ParsedRequest) => void;
    /** The command line is shown. */
    open?: boolean;
    /** Stop or start a node. Returns an error message, or null. */
    onpower: (node: number, stop: boolean) => string | null;
    /** `riak admin cluster join|leave|plan|commit` on a node. */
    onadmin?: (node: number, action: string, target: number | null) => { ok: boolean; text: string };
  } = $props();

  interface Entry {
    command: string;
    req: number | null;
    include: boolean;
    output: string;
    error: boolean;
  }

  let input = $state("");
  let log = $state<Entry[]>([]);
  let logEl: HTMLElement | undefined = $state();
  let inputEl: HTMLTextAreaElement | undefined = $state();

  function scroll() {
    queueMicrotask(() => logEl?.scrollTo({ top: logEl.scrollHeight }));
  }

  // "node3$ riak stop": run riak stop on node3, as an operator would.
  const POWER = /^node(\d+)\$\s*riak\s+(stop|start)$/;
  // "node6$ riak admin cluster join node1": membership changes, staged and
  // then committed, as in Riak. "riak-admin" also works.
  const ADMIN = /^node(\d+)\$\s*riak[\s-]admin\s+cluster\s+(join|leave|plan|commit)(?:\s+node(\d+))?$/;

  function run(text: string) {
    for (const line of text.split("\n")) runLine(line);
  }

  function runLine(command: string) {
    command = command.trim();
    if (!command) return;
    const admin = ADMIN.exec(command);
    if (admin) {
      const node = Number(admin[1]) - 1;
      const target = admin[3] ? Number(admin[3]) - 1 : null;
      const r = node >= 0 && node < nodes ? onadmin(node, admin[2], target) : { ok: false, text: `There is no node${node + 1}.` };
      log.push({ command, req: null, include: false, output: r.text, error: !r.ok });
      scroll();
      return;
    }
    const power = POWER.exec(command);
    if (power) {
      const node = Number(power[1]) - 1;
      const error = node >= 0 && node < nodes ? onpower(node, power[2] === "stop") : `There is no node${node + 1}.`;
      log.push({ command, req: null, include: false, output: error ?? "ok", error: error !== null });
      scroll();
      return;
    }
    const parsed = parseCommand(command, nodes);
    if (!parsed.ok) {
      log.push({ command, req: null, include: false, output: parsed.error, error: true });
    } else {
      const req = cluster.submit(parsed.request);
      log.push({ command, req, include: parsed.request.include, output: "Waiting for the reply.", error: false });
      onsubmit(req, parsed.request);
    }
    scroll();
  }

  /** Run commands from outside, such as the buttons on a node card. */
  export function runCommands(commands: string[]) {
    for (const c of commands) runLine(c);
  }

  /** A summary line for many requests at once (a batch, traffic or a
   *  scenario): one line in the log with a running count of the replies. */
  export function summary(label: string): { sent: (n?: number) => void; got: (status: number) => void; note: (text: string) => void } {
    const entry: Entry = { command: label, req: null, include: false, output: "", error: false };
    log.push(entry);
    const i = log.length - 1;
    let sent = 0;
    let extra = "";
    const counts = new Map<number, number>();
    const show = () => {
      const got = [...counts.values()].reduce((a, b) => a + b, 0);
      const parts = [...counts.entries()].sort((a, b) => a[0] - b[0]).map(([s, n]) => `${n.toLocaleString()} × ${s === 0 ? "refused" : s}`);
      const waiting = sent - got;
      log[i].output = [`${sent.toLocaleString()} sent`, ...parts, waiting > 0 ? `${waiting.toLocaleString()} waiting` : "", extra].filter(Boolean).join(" · ");
      log[i].error = [...counts.keys()].some((s) => s === 0 || s >= 500);
    };
    show();
    scroll();
    return {
      sent: (n = 1) => {
        sent += n;
        show();
      },
      got: (status) => {
        counts.set(status, (counts.get(status) ?? 0) + 1);
        show();
      },
      note: (text) => {
        extra = text;
        show();
      },
    };
  }

  /** Called by the page when it replaces the cluster: replies to earlier
   *  commands can no longer arrive, and request ids start again. */
  export function retire() {
    for (const e of log) {
      if (e.req !== null && e.output === "Waiting for the reply.") e.output = "No reply. The cluster was replaced.";
      e.req = null;
    }
  }

  /** Called by the page when a reply reaches the client. */
  export function receive(r: HttpResponse) {
    const entry = log.find((e) => e.req === r.req);
    if (!entry) return;
    if (r.status === 0) {
      // Nothing listens on a stopped node.
      const host = entry.command.match(/node\d+/)?.[0] ?? "the node";
      entry.output = `curl: (7) Failed to connect to ${host} port 8098: Connection refused`;
      entry.error = true;
      scroll();
      return;
    }
    const lines: string[] = [];
    if (entry.include) {
      lines.push(`HTTP/1.1 ${r.status} ${r.reason}`);
      for (const [k, v] of r.headers) lines.push(`${k}: ${v}`);
      lines.push("");
    } else if (r.status >= 400) {
      lines.push(`HTTP ${r.status} ${r.reason}`);
    }
    // A long body (a list of keys) is cut, so the console stays readable.
    const LONG = 600;
    const body = r.body.length > LONG ? `${r.body.slice(0, LONG)}… (${r.body.length.toLocaleString()} characters)` : r.body;
    if (body) lines.push(body.replace(/\n$/, ""));
    if (!entry.include && !r.body && r.status < 400) lines.push(`${r.status} ${r.reason}. No body.`);
    entry.output = lines.join("\n");
    entry.error = r.status >= 400;
    scroll();
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      run(input);
      input = "";
    }
  }
</script>

{#if open}
<section class="repl" aria-label="Command line">
  <div class="log" bind:this={logEl} aria-live="polite">
    {#each log as entry, i (i)}
      <pre class="command">$ {entry.command}</pre>
      <pre class="output" class:error={entry.error}>{entry.output}</pre>
    {:else}
      <p class="hint">
        Each command is an HTTP request. Use curl. The host localhost is the load balancer, which sends each request to the next node in turn. The hosts node1, node2 and so on are the nodes themselves.
        You can also type a method and a path, for example <code>GET /ping</code>. To stop or start a node, type for example <code>node3$ riak stop</code> or <code>node3$ riak start</code>. To change the cluster, use <code>riak admin cluster join</code>, <code>leave</code>, <code>plan</code> and <code>commit</code>.
      </p>
    {/each}
  </div>
  <label class="field">
    <span class="field-label">Command</span>
    <textarea
      bind:this={inputEl}
      bind:value={input}
      onkeydown={keydown}
      rows="3"
      spellcheck="false"
      autocapitalize="off"
      autocomplete="off"
      placeholder="curl localhost:8098/ping"></textarea>
  </label>
  <div class="actions">
    <span class="field-label">Enter: run. Shift+Enter: new line.</span>
    <span class="spacer"></span>
    <Button
      variant="tonal"
      onclick={() => {
        run(input);
        input = "";
      }}
      disabled={!input.trim()}>Run</Button
    >
  </div>
</section>
{/if}

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .field-label {
    font: var(--type-label-medium);
    letter-spacing: var(--tracking-label);
    color: var(--md-sys-color-on-surface-variant);
  }
  p {
    margin: 0;
    font: var(--type-body-medium);
    color: var(--md-sys-color-on-surface-variant);
  }
  pre {
    margin: 0;
    font: var(--type-code);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .spacer {
    flex: 1;
  }
  .repl {
    margin-top: var(--space-2);
  }
  .log {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-height: var(--size-log-max);
    overflow-y: auto;
    padding: var(--space-3);
    border-radius: var(--shape-small);
    background: var(--md-sys-color-surface-container-lowest);
  }
  .command {
    color: var(--md-sys-color-on-surface-variant);
  }
  .output {
    margin-bottom: var(--space-2);
  }
  .output.error {
    color: var(--md-sys-color-error);
  }
  .hint {
    font: var(--type-body-small);
  }
  .hint code {
    font: var(--type-code-small);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  textarea {
    min-height: var(--size-input-min);
    padding: var(--space-3);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline);
    border-radius: var(--shape-extra-small);
    background: none;
    font: var(--type-code);
    resize: vertical;
  }
  textarea:focus-visible {
    outline: none;
    border-color: var(--md-sys-color-primary);
  }
</style>
