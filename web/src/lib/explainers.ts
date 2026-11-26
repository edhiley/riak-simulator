// Text for the explainer popups. Text follows the writing rules in DESIGN.md.
// A point marked plain is shown without glossary hovers, because it uses a
// word in a different sense than the glossary does.

export interface Explainer {
  id: string;
  title: string;
  points: { text: string; plain?: boolean }[];
  /** Groups of links shown under the points, each with a heading. */
  links?: { heading: string; items: { label: string; href: string; note?: string }[] }[];
  /** A short line under the links, in smaller text. */
  footnote?: string;
}

export const about: Explainer = {
  id: "about",
  title: "How the simulator works",
  points: [
    {
      text: "No Riak runs here. A model of an OpenRiak KV cluster runs in your browser, and the page draws what it does.",
    },
    {
      text: "The core is written in Rust. Each node and each vnode is an actor, and each vnode has a mailbox. Messages travel on a simulated network, and a simulation clock sets when each one arrives. The core has no real clock and uses a seeded random generator, so the same inputs always give the same run.",
    },
    {
      text: "The core is compiled to WebAssembly. The page (Svelte and TypeScript) sends your inputs to it: requests, and nodes that join, leave, stop or start again. On each frame, the page reads a snapshot of the cluster and draws every message that is in flight.",
    },
    {
      text: "The model follows the OpenRiak 3.4.1 source: the put and get FSMs, the vnodes and their proxies, the ports, hinted handoff, ownership changes, key folds and the leveled backend. The simulator copies no Riak code. It is a new program that does the same steps, with the same names and default settings.",
    },
    {
      text: "Key placement is checked against a real OpenRiak 3.4.1 node. The node calculated the hash and the preference list for 1,530 bucket and key pairs, and the core gives the same result for each one.",
    },
    {
      text: "Some things are simplified. Times are not real times. Partitions are given to nodes in turn, not by Riak's claim algorithm. Leveled's sizes are its defaults divided by 1,000, so that a few hundred keys fill several levels. Bitcask and the other backends, read repair, active anti-entropy and request timeouts are not modelled.",
    },
    {
      text: "Riak was made by Basho Technologies. bet365 released all of its code as open source in 2017. The OpenRiak community now develops it, as a working group of the Erlang Ecosystem Foundation. Leveled began as Martin Sumner's project. The code is under the Apache License 2.0. Thanks to everyone who has worked on these repositories.",
      plain: true,
    },
  ],
  links: [
    {
      heading: "Sources",
      items: [
        { label: "This simulator's source code", href: "https://github.com/edhiley/riak-simulator", note: "on GitHub, under the Apache License 2.0" },
        { label: "OpenRiak", href: "https://github.com/OpenRiak", note: "the project and all its repositories" },
        { label: "riak", href: "https://github.com/OpenRiak/riak", note: "the release; this simulator follows tag riak-3.4.1" },
        { label: "riak_kv", href: "https://github.com/OpenRiak/riak_kv", note: "the put and get FSMs, the KV vnode" },
        { label: "riak_core", href: "https://github.com/OpenRiak/riak_core", note: "the ring, vnodes, proxies, handoff" },
        { label: "riak_api", href: "https://github.com/OpenRiak/riak_api", note: "the client interfaces" },
        { label: "leveled", href: "https://github.com/OpenRiak/leveled", note: "the backend" },
        { label: "OpenRiak documentation", href: "https://openriak.github.io/riak/" },
        { label: "OpenRiak discussions", href: "https://github.com/orgs/OpenRiak/discussions" },
        { label: "Erlang Ecosystem Foundation", href: "https://erlef.org/" },
      ],
    },
    {
      heading: "Licence",
      items: [
        { label: "LICENSE", href: "./LICENSE.txt", note: "this simulator is under the Apache License 2.0" },
        { label: "NOTICE", href: "./NOTICE.txt", note: "the credits for the OpenRiak sources" },
        { label: "Third-party licences", href: "./THIRD-PARTY-LICENSES.txt", note: "the fonts, icons and libraries in this page" },
      ],
    },
  ],
  footnote:
    "This simulator is a learning tool. It is not part of OpenRiak, and the OpenRiak project and its maintainers do not endorse it.",
};

export const cap: Explainer = {
  id: "cap",
  title: "The CAP theorem",
  points: [
    {
      text: "CAP is the first letter of three properties. Consistency (C): every read gets the newest write, or an error. Availability (A): every request gets a reply that is not an error. Partition tolerance (P): the database continues to work when the network splits.",
      plain: true,
    },
    {
      text: "Eric Brewer stated the idea in 2000, and Seth Gilbert and Nancy Lynch proved it in 2002. The usual short form is: pick two of the three.",
      plain: true,
    },
    {
      text: "But networks do split, so a database on more than one machine must have P. Thus the real choice is between C and A, and only while the network is split. CA is possible only on one machine, such as a single PostgreSQL server.",
      plain: true,
    },
    {
      text: "A network can split. Then the nodes on one side cannot send messages to the nodes on the other side. Each side can still get requests from clients.",
    },
    {
      text: "The CAP theorem calls a network split a network partition. This is a different thing from the partitions of the ring.",
      plain: true,
    },
    {
      text: "During a network split, a database must choose. It can give consistency (CP) and send an error when it cannot reach the newest write, as CockroachDB, etcd and Google Spanner do. Or it can give availability (AP) and reply with the data that it has, as Riak, Cassandra and ScyllaDB do. It cannot give both, because one side cannot see the writes on the other side.",
    },
    {
      text: "Riak chooses availability. Any node takes any request. If a vnode with a copy cannot be reached, Riak saves the copy in a fallback vnode on another node. So reads and writes continue on both sides of the split.",
    },
    {
      text: "The cost is that the two sides can take different writes to the same key. Riak uses the vclocks to find this conflict. With the default bucket properties, Riak keeps the version with the newest timestamp. If allow_mult is true, Riak keeps both versions as siblings, and your application chooses one.",
    },
    {
      text: "After the split, Riak sends the copies in fallback vnodes back to their usual vnodes. This is hinted handoff. Riak also repairs old copies when it reads them, and in the background. Then all copies are the same again. This is eventual consistency.",
    },
    {
      text: "You can change the choice for each request. With pr and pw, a request fails when it cannot reach enough copies in their usual vnodes. This gives up some availability to get more consistency.",
    },
    {
      text: "There is a choice even when no network has split. If R or W is larger, a request waits for more copies. The answer is newer, but it comes later.",
    },
    {
      text: "Riak follows the Dynamo design that Amazon described in 2007, so it chooses availability. DynamoDB, Amazon's later service, has the name but a different design: each partition has a leader copy. A DynamoDB read is eventually consistent by default and can come from any copy. A strongly consistent read must go to the leader, so it can fail when the leader cannot be reached.",
    },
  ],
};

export const nodeCount: Explainer = {
  id: "nodes",
  title: "How many nodes?",
  points: [
    {
      text: "Riak keeps 3 copies of each object by default (n_val is 3). With fewer than 3 nodes, some copies are on the same node. If that node fails, you lose more than one copy. Try 1 or 2 nodes here and look at the outlined tiles.",
    },
    {
      text: "With 3 nodes, one failed node leaves 2 nodes for 3 copies. Riak must put 2 copies on one node until the failed node comes back.",
    },
    {
      text: "The Riak documentation recommends 5 or more nodes for production. Then, when one node fails, there are still enough other nodes to keep 3 copies on 3 different nodes.",
    },
    {
      text: "More nodes give more disk space and can do more requests at the same time. When one node fails, it has a smaller share of the data, so the other nodes have less extra work.",
    },
    {
      text: "More nodes also cost more, and with more nodes it is more probable that one of them has a problem at a given time. Riak is designed for this: one failed node does not stop the cluster.",
    },
    {
      text: "You can add nodes to a cluster or remove nodes from it while it runs. Riak then moves some partitions to other nodes. Try it with Add a node.",
    },
  ],
};

export const ringSize: Explainer = {
  id: "ring-size",
  title: "How many partitions?",
  points: [
    {
      text: "ring_size is the number of partitions. It must be a power of 2. The default is 64. This simulator lets you choose from 8 to 64, so that you can see each partition. Large clusters use 256, 512 or more.",
    },
    {
      text: "Each partition is a vnode. Riak gives each node an equal share of the vnodes. If the number of partitions does not divide evenly, some nodes have one more vnode than others, so they do a little more work. Try 32 partitions on 5 nodes.",
    },
    {
      text: "With too few partitions, Riak cannot spread the data evenly over many nodes. With 8 partitions and 8 nodes, each node has only one vnode. If you add a ninth node, it gets no data.",
    },
    {
      text: "With too many partitions, each node runs many vnodes. Each vnode has its own process and storage files. This uses more memory and more file handles on each node.",
    },
    {
      text: "Choose ring_size for the largest cluster that you expect. The Riak documentation recommends 10 to 50 partitions on each node.",
    },
    {
      text: "You set ring_size when you make the cluster. It is possible to change it later, but this is slow, because Riak must move most of the data.",
    },
    {
      text: "With 3 nodes, a power of 2 never divides by 3. So Riak cannot always put neighbouring partitions on 3 different nodes, and some keys have 2 copies on one node. This is one more reason to use 5 or more nodes.",
    },
  ],
};

export const history: Explainer = {
  id: "history",
  title: "Where Riak comes from",
  points: [
    {
      text: "2007: Amazon publishes a paper about Dynamo, the key-value store behind its shopping cart. Dynamo has no leader node. It places keys with a hash ring, keeps copies on several nodes, takes writes when nodes fail (sloppy quorum and hinted handoff), and uses vector clocks to find conflicting versions.",
      plain: true,
    },
    {
      text: "2008: Basho Technologies is founded and builds Riak, an open-source database that follows the Dynamo design, written in Erlang. Riak is first released in 2009. Everything this simulator shows comes from that design: the ring, vnodes, R and W, fallback vnodes and hinted handoff.",
      plain: true,
    },
    {
      text: "Other databases took the same ideas. Facebook released Cassandra as open source in 2008: Dynamo's ring and replication with a Bigtable-style data model. LinkedIn released Project Voldemort in 2009.",
      plain: true,
    },
    {
      text: "2014: Riak 2.0 adds data types (CRDTs) such as counters and sets, so that copies merge without siblings.",
      plain: true,
    },
    {
      text: "2017: Basho goes into receivership. bet365, a Riak user, buys its assets and releases all of the code as open source, including the features that were only in the paid version. NHS Digital, which runs the NHS Spine on Riak, funds further development.",
      plain: true,
    },
    {
      text: "2024: the OpenRiak community forms to carry on development, as a working group of the Erlang Ecosystem Foundation. This simulator follows the OpenRiak 3.4 source.",
      plain: true,
    },
    {
      text: "DynamoDB is not Dynamo. Amazon launched DynamoDB as a managed service in 2012. Its 2022 paper describes a different design: each partition has a leader copy, kept in step with Multi-Paxos. Writes and strongly consistent reads go to the leader, and any copy can serve an eventually consistent read.",
      plain: true,
    },
    {
      text: "The main difference, in short. Riak: no leader, any copy takes a write, and conflicts are found later with vector clocks (siblings or data types). DynamoDB: one leader per partition orders the writes, so conflicts cannot happen, but a partition that loses its leader cannot take writes until a new leader is chosen. Cassandra: no leader, like Riak, but conflicting writes are settled by timestamp (the last write wins).",
      plain: true,
    },
  ],
};
