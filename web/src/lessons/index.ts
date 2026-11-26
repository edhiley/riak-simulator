// Guided lessons are data: each step has paragraphs of text and one command.
// Text follows the writing rules in DESIGN.md.

export interface LessonStep {
  title: string;
  /** Why this matters for reliability. Shown in the What is happening box before the command runs. */
  intro: string[];
  /** Show a link to the CAP theorem popup under the intro. */
  cap?: boolean;
  /** Run stays on after the command has run, because the part asks to run it again. */
  repeat?: boolean;
  text: string[];
  command: string;
}

export interface Lesson {
  id: string;
  title: string;
  steps: LessonStep[];
}

export const lessons: Lesson[] = [
  {
    id: "write-and-read",
    title: "Reading and writing objects in Riak",
    steps: [
      {
        title: "Writing an object",
        intro: [
          "One server can fail. Its disk can break, or it can lose power or its network. If an object is on one server only, you cannot read it while that server is down, and you lose it if the disk breaks.",
          "So Riak keeps 3 copies of every object, in 3 vnodes. The number of copies is n_val, and 3 is the default. When the cluster has 3 or more nodes, the 3 vnodes are on 3 different nodes. If one node fails, 2 copies are still there, and Riak can still read and write the object.",
          "Copies on many nodes keep the data available when nodes fail. But copies can also be different from each other for a short time. Every distributed database must choose between these two things. The CAP theorem describes this choice.",
          "Run the command to see where the 3 copies go.",
        ],
        cap: true,
        text: [
          "Riak calculates the hash of the bucket and the key. The hash gives 3 partitions, and Riak keeps a copy of the object in each. Each partition is a vnode: one of the coloured tiles inside a node. To see how the hash gives the partitions, select What are partitions? under the nodes.",
          "Run the command. localhost is a load balancer in front of the nodes. It sends each request to the next node in turn. Each label on the screen is a message. The box on the right, What is happening, explains each step. The simulator stops after each step. Select Continue to go on.",
          "One node coordinates the write. It replies when 2 copies confirm the write. The next steps explain the details.",
        ],
        command: `curl -X PUT localhost:8098/buckets/users/keys/alice -H 'Content-Type: text/plain' -d 'Alice'`,
      },
      {
        title: "Another write to a different key",
        intro: [
          "Riak has no central list of where each key is kept. A list like that would be one more thing that could fail, and every request would have to wait for it.",
          "Instead, every node calculates the 3 places from the key itself. Any node can find any key, even when other nodes are down.",
        ],
        text: [
          "Run the command. It writes the key bob. bob has a different hash from alice, so different tiles are outlined. They are on 3 different nodes.",
          "Select Edit to change bob to another key, for example your own name. Then run it from the console.",
        ],
        command: `curl -X PUT localhost:8098/buckets/users/keys/bob -H 'Content-Type: text/plain' -d 'Bob'`,
      },
      {
        title: "Any node can take a write",
        intro: [
          "Riak has no leader node. Every node can take every request, so no single node can stop all writes when it fails.",
          "The node that takes a request does not need a copy of the key. If it has no copy, it passes the write to a node that has one.",
        ],
        text: [
          "Run the command 2 or 3 times. Each time, the load balancer sends the request to the next node. When that node has no copy of alice, it passes the write to a node that has one. That node is the coordinator. It sends now_executing back, to say that the write has started.",
          "The coordinator saves its own copy first. Its vnode sends a write acknowledgement (w ACK) when the write arrives, and a durable-write acknowledgement (dw ACK) when the copy is saved. Then the coordinator sends the write to the other 2 vnodes.",
          "W is the number of write acknowledgements the coordinator needs. DW is the number of durable-write acknowledgements it needs. Both are 2 by default. Look for acknowledgements that arrive after the reply.",
        ],
        command: `curl -X PUT localhost:8098/buckets/users/keys/alice -H 'Content-Type: text/plain' -d 'Alice Smith'`,
        repeat: true,
      },
      {
        title: "Writing while a node is stopped",
        intro: [
          "A node can stop at any time. Its server can crash, it can restart for an upgrade, or its network can fail. Writes must go on.",
          "When a node with a copy is stopped, Riak still keeps 3 copies. It puts the missing copy in a fallback vnode on the next running node around the ring. The fallback vnode keeps the copy until the stopped node is back.",
          "Riak chooses to keep taking writes when a node is down. This is the availability side of the choice that the CAP theorem describes.",
        ],
        cap: true,
        text: [
          "The command first stops node3, as if its server failed. node3 has one of the 3 copies of alice.",
          "Then it writes alice. The load balancer skips node3. The copy for node3 goes to a fallback vnode on another node. Its tile has a dashed edge. The write succeeds.",
        ],
        command: `node3$ riak stop\ncurl -X PUT localhost:8098/buckets/users/keys/alice -H 'Content-Type: text/plain' -d 'Alice S.'`,
      },
      {
        title: "Starting the node again",
        intro: [
          "When a stopped node starts again, its vnodes still have the copies they had before it stopped. Those copies can be old.",
          "So the fallback vnode sends its copy back to the usual vnode on that node, and then deletes its own copy. This is hinted handoff. After it, every copy is on its usual vnode again.",
        ],
        text: [
          "The command starts node3 again.",
          "Watch the handoff message go from the fallback vnode to node3. node3 keeps the newer version of alice. Then the fallback vnode deletes its copy, and its tile goes.",
        ],
        command: `node3$ riak start`,
      },
      {
        title: "Reading the object",
        intro: [
          "A read asks all 3 copies but waits for only R of them. With R = 2, a read still works when one node is down.",
          "W + R is 4, which is more than the 3 copies. So when no node has failed, the copies that answer a read include at least one that took the newest write.",
          "When a network splits, Riak still replies on both sides. Then a read can miss the newest write, until Riak makes the copies the same again. The CAP theorem explains why.",
        ],
        cap: true,
        text: [
          "The node that gets a read coordinates it. It does not send the read to another node. It sends HEAD to the 3 vnodes. A HEAD answer has the vclock but not the value.",
          "R is the number of answers the coordinator waits for. R is 2 by default. Then the coordinator gets the value from one vnode with the newest vclock.",
        ],
        command: `curl -i node2:8098/buckets/users/keys/alice`,
      },
    ],
  },
];

/** Parts of a lesson are lettered: A, B, C. */
export const partLetter = (i: number) => String.fromCharCode(65 + i);

/** "Part a: Writing an object", for a step of any lesson. */
export function partLabel(step: LessonStep): string {
  for (const lesson of lessons) {
    const i = lesson.steps.indexOf(step);
    if (i >= 0) return `Part ${partLetter(i)}: ${step.title}`;
  }
  return step.title;
}
