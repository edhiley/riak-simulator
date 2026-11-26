// Example commands for the console's help list. Each one is accepted by the
// console as it is; curl.test.ts checks the curl ones.

export interface Example {
  label: string;
  command: string;
}

export const examples: { group: string; items: Example[] }[] = [
  {
    group: "Keys and values",
    items: [
      { label: "Ping a node through the load balancer", command: "curl localhost:8098/ping" },
      { label: "Write a value", command: "curl -X PUT localhost:8098/buckets/users/keys/alice -H 'Content-Type: text/plain' -d 'Alice'" },
      { label: "Read it", command: "curl localhost:8098/buckets/users/keys/alice" },
      { label: "Read it with the headers, from node3", command: "curl -i node3:8098/buckets/users/keys/alice" },
      { label: "Read with R=1: the first reply wins", command: "curl 'localhost:8098/buckets/users/keys/alice?r=1'" },
      { label: "Write only if 2 copies are on their usual vnodes", command: "curl -X PUT 'localhost:8098/buckets/users/keys/bob?pw=2' -H 'Content-Type: text/plain' -d 'Bob'" },
      { label: "Delete it", command: "curl -X DELETE localhost:8098/buckets/users/keys/alice" },
      { label: "List the keys in a bucket", command: "curl 'localhost:8098/buckets/users/keys?keys=true'" },
    ],
  },
  {
    group: "Siblings and data types",
    items: [
      {
        label: "Make two siblings: two writes with no vclock",
        command:
          "curl -X PUT localhost:8098/types/carts/buckets/carts/keys/bob -H 'Content-Type: text/plain' -d 'milk'\ncurl -X PUT localhost:8098/types/carts/buckets/carts/keys/bob -H 'Content-Type: text/plain' -d 'bread'",
      },
      { label: "Read the siblings", command: "curl -i -H 'Accept: multipart/mixed' localhost:8098/types/carts/buckets/carts/keys/bob" },
      { label: "Add 1 to a counter", command: "curl -X POST localhost:8098/types/counters/buckets/stats/datatypes/visits -H 'Content-Type: application/json' -d '{\"increment\": 1}'" },
      { label: "Read the counter", command: "curl localhost:8098/types/counters/buckets/stats/datatypes/visits" },
      { label: "Add an element to a set", command: "curl -X POST localhost:8098/types/sets/buckets/fruit/datatypes/basket -H 'Content-Type: application/json' -d '{\"add\": \"apple\"}'" },
      { label: "Read the set", command: "curl localhost:8098/types/sets/buckets/fruit/datatypes/basket" },
    ],
  },
  {
    group: "Nodes and the cluster",
    items: [
      { label: "Stop node3", command: "node3$ riak stop" },
      { label: "Start node3: fallback vnodes hand their copies back", command: "node3$ riak start" },
      { label: "Add node6 to the cluster", command: "node6$ riak admin cluster join node1\nnode1$ riak admin cluster plan\nnode1$ riak admin cluster commit" },
      { label: "Make node6 leave", command: "node6$ riak admin cluster leave\nnode1$ riak admin cluster plan\nnode1$ riak admin cluster commit" },
    ],
  },
];
