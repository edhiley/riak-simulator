<script lang="ts">
  // Live mode controls: example commands that go into the console, and a
  // switch for each node. Commands run through the console, so the user
  // sees the curl command for each button.
  let {
    members,
    maxNodes,
    leaving,
    down,
    onrun,
    ontoggle,
  }: {
    members: number[];
    maxNodes: number;
    leaving: number[];
    down: number[];
    onrun: (commands: string[]) => void;
    ontoggle: (node: number, stop: boolean) => void;
  } = $props();

  const KEYS = 9;
  let next = $state(1);
  // The key the last write used, so Read and Delete find something.
  let last = $state<number | null>(null);

  const put = (k: number) => `curl -X PUT localhost:8098/buckets/users/keys/user${k} -H 'Content-Type: text/plain' -d 'value ${k}'`;
  const key = () => last ?? 1;

  function write() {
    const command = put(next);
    last = next;
    next = (next % KEYS) + 1;
    onrun([command]);
  }

  // Many writes to new random keys, to fill the cluster with data. They are
  // sent a little apart, so each is its own pulse, and within 3 seconds.
  const MAX_BURST = 500;
  const BURST_SPREAD_MS = 3000;
  let count = $state(10);
  const randomKey = () => `key-${Math.random().toString(36).slice(2, 8)}`;

  function writeMany() {
    const n = Math.max(1, Math.min(MAX_BURST, Math.round(count) || 1));
    count = n;
    const gap = Math.min(150, BURST_SPREAD_MS / n);
    for (let i = 0; i < n; i++) {
      const k = randomKey();
      const command = `curl -X PUT localhost:8098/buckets/data/keys/${k} -H 'Content-Type: text/plain' -d 'value of ${k}'`;
      setTimeout(() => onrun([command]), i * gap);
    }
  }

  // Data types (CRDTs), in the bucket types counters and sets.
  const COUNTER = "localhost:8098/types/counters/buckets/stats/datatypes/visits";
  const SET = "localhost:8098/types/sets/buckets/fruit/datatypes/basket";
  const FRUIT = ["apple", "pear", "plum", "fig", "lime"];
  let fruit = $state(0);
  const json = (body: string) => `-H 'Content-Type: application/json' -d '${body}'`;

  function addFruit() {
    const f = FRUIT[fruit];
    fruit = (fruit + 1) % FRUIT.length;
    onrun([`curl -X POST ${SET} ${json(`{"add": "${f}"}`)}`]);
  }

  let stopped = $derived(new Set(down));
  let going = $derived(new Set(leaving));
  let lastRunning = $derived(members.filter((n) => !stopped.has(n)).length <= 1);
  // The lowest free node number joins next.
  let free = $derived(Array.from({ length: maxNodes }, (_, n) => n).find((n) => !members.includes(n)) ?? null);
  const admin = (n: number, rest: string) => `node${n + 1}$ riak admin cluster ${rest}`;

  /** Join a new node: stage, show the plan, commit. */
  function addNode() {
    if (free === null) return;
    const to = members[0];
    onrun([admin(free, `join node${to + 1}`), admin(to, "plan"), admin(to, "commit")]);
  }

  function leaveNode(n: number) {
    onrun([admin(n, "leave"), admin(n, "plan"), admin(n, "commit")]);
  }
</script>

<section class="live" aria-labelledby="live-title">
  <h2 id="live-title">Live cluster</h2>
  <p class="supporting">Send requests and stop or start nodes. Each button puts its command in the console.</p>

  <div class="group" role="group" aria-label="Example commands">
    <button type="button" class="chip" onclick={write}>Write</button>
    <button type="button" class="chip" onclick={() => onrun([`curl localhost:8098/buckets/users/keys/user${key()}`])}>Read</button>
    <button type="button" class="chip" onclick={() => onrun([`curl -X DELETE localhost:8098/buckets/users/keys/user${key()}`])}>Delete</button>
    <button type="button" class="chip" onclick={() => onrun(["curl localhost:8098/ping"])}>Ping</button>
    <button
      type="button"
      class="chip"
      title="Refuse the write unless 2 copies are on their usual vnodes"
      onclick={() => onrun([`curl -X PUT 'localhost:8098/buckets/users/keys/user${key()}?pw=2' -H 'Content-Type: text/plain' -d 'safe ${key()}'`])}>Write with pw=2</button
    >
  </div>
  <div class="group burst">
    <label class="count">
      <span class="sr">Number of keys</span>
      <input type="number" min="1" max={MAX_BURST} bind:value={count} />
    </label>
    <button type="button" class="chip" onclick={writeMany}>Write {count} random keys</button>
  </div>

  <h3>Data types (CRDTs)</h3>
  <div class="group" role="group" aria-label="Data type commands">
    <button type="button" class="chip" onclick={() => onrun([`curl -X POST ${COUNTER} ${json('{"increment": 1}')}`])}>Counter +1</button>
    <button type="button" class="chip" onclick={() => onrun([`curl ${COUNTER}`])}>Read counter</button>
    <button type="button" class="chip" onclick={addFruit}>Add to set</button>
    <button type="button" class="chip" onclick={() => onrun([`curl -X POST ${SET} ${json(`{"remove": "${FRUIT[0]}"}`)}`])}>Remove {FRUIT[0]}</button>
    <button type="button" class="chip" onclick={() => onrun([`curl ${SET}`])}>Read set</button>
  </div>

  <div class="nodes-head">
    <h3>Nodes</h3>
    <button type="button" class="chip" disabled={free === null} onclick={addNode}>Add a node</button>
  </div>
  <div class="nodes" role="group" aria-label="Nodes">
    {#each members as n (n)}
      {@const on = !stopped.has(n)}
      <div class="switch-row">
        <span class="dot" style:background="var(--node-{n})"></span>
        <span class="name">node{n + 1}</span>
        <span class="state">{going.has(n) ? "leaving" : on ? "running" : "stopped"}</span>
        <button
          type="button"
          class="leave"
          disabled={going.has(n) || !on || members.length - going.size <= 1}
          onclick={() => leaveNode(n)}>Leave</button
        >
        <button
          type="button"
          role="switch"
          aria-checked={on}
          aria-label="node{n + 1} running"
          class="switch"
          class:on
          disabled={on && lastRunning}
          onclick={() => ontoggle(n, on)}><span class="thumb"></span></button
        >
      </div>
    {/each}
  </div>
</section>

<style>
  .live {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  h2 {
    margin: 0;
    font: var(--type-title-large);
  }
  h3 {
    margin: var(--space-1) 0 0;
    font: var(--type-title-small);
  }
  .supporting {
    margin: 0;
    font: var(--type-body-medium);
    color: var(--md-sys-color-on-surface-variant);
  }
  .group {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  /* Material 3 assist chip. */
  .chip {
    min-height: var(--size-target);
    padding: 0 var(--space-4);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline);
    border-radius: var(--shape-small);
    background: none;
    color: var(--md-sys-color-on-surface);
    font: var(--type-label-large);
    cursor: pointer;
  }
  .chip:hover {
    background: var(--md-sys-color-surface-container-highest);
  }
  .burst {
    align-items: center;
  }
  .count input {
    width: calc(var(--size-target) * 2);
    min-height: var(--size-target);
    padding: 0 var(--space-2);
    border: var(--stroke-hairline) solid var(--md-sys-color-outline);
    border-radius: var(--shape-small);
    background: var(--md-sys-color-surface-container-lowest);
    color: var(--md-sys-color-on-surface);
    font: var(--type-label-large);
  }
  .sr {
    position: absolute;
    width: var(--stroke-hairline);
    height: var(--stroke-hairline);
    overflow: hidden;
    clip-path: inset(50%);
  }
  .nodes-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .leave {
    min-height: var(--size-target);
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--shape-full);
    background: none;
    color: var(--md-sys-color-primary);
    font: var(--type-label-large);
    cursor: pointer;
  }
  .leave:disabled {
    opacity: var(--dim);
    cursor: default;
  }
  .chip:disabled {
    opacity: var(--dim);
    cursor: default;
  }
  .nodes {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .switch-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--size-target);
  }
  .dot {
    width: var(--space-3);
    height: var(--space-3);
    border-radius: var(--shape-full);
  }
  .name {
    font: var(--type-label-large);
  }
  .state {
    flex: 1;
    font: var(--type-body-small);
    color: var(--md-sys-color-on-surface-variant);
  }
  /* Material 3 switch. */
  .switch {
    position: relative;
    width: calc(var(--size-target) + var(--space-3));
    height: calc(var(--space-8) - var(--space-1));
    padding: 0;
    border: var(--stroke-thick) solid var(--md-sys-color-outline);
    border-radius: var(--shape-full);
    background: var(--md-sys-color-surface-container-highest);
    cursor: pointer;
  }
  .switch.on {
    border-color: var(--md-sys-color-primary);
    background: var(--md-sys-color-primary);
  }
  .thumb {
    position: absolute;
    top: 50%;
    left: var(--space-1);
    width: var(--space-4);
    height: var(--space-4);
    border-radius: var(--shape-full);
    background: var(--md-sys-color-outline);
    transform: translateY(-50%);
    transition: left var(--duration-short) var(--motion-standard);
  }
  .switch.on .thumb {
    left: calc(100% - var(--space-6));
    width: var(--space-5);
    height: var(--space-5);
    background: var(--md-sys-color-on-primary);
  }
  .switch:disabled {
    opacity: var(--dim);
    cursor: default;
  }
</style>
