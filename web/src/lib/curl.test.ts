import { test } from "node:test";
import assert from "node:assert/strict";
import { parseCommand, tokenize } from "./curl.ts";

const ok = (input: string, nodes = 5) => {
  const r = parseCommand(input, nodes);
  if (!r.ok) throw new Error(r.error);
  return r.request;
};
const err = (input: string, nodes = 5) => {
  const r = parseCommand(input, nodes);
  assert.equal(r.ok, false, `expected an error for ${input}`);
  return (r as { error: string }).error;
};

test("tokenizes quotes, escapes and line continuations", () => {
  assert.deepEqual(tokenize(`curl -H 'A: b c' -d "say \\"hi\\"" \\\n x`), ["curl", "-H", "A: b c", "-d", 'say "hi"', "x"]);
  assert.equal(tokenize("curl 'oops"), "A single quote is not closed.");
});

test("a PUT with a header and a body", () => {
  const r = ok(`curl -X PUT localhost:8098/buckets/users/keys/alice -H 'Content-Type: text/plain' -d 'Alice'`);
  assert.deepEqual(r, {
    node: null, method: "PUT", path: "/buckets/users/keys/alice",
    headers: [["Content-Type", "text/plain"]], body: "Alice", include: false,
  });
});

test("host names the node or the load balancer", () => {
  assert.equal(ok("curl http://node3:8098/ping").node, 2);
  assert.equal(ok("curl 127.0.0.1/ping").node, null);
  assert.equal(ok("curl lb:8098/ping").node, null);
  assert.match(err("curl node6:8098/ping"), /There is no node6 in a cluster of 5/);
  assert.match(err("curl example.com/ping"), /Unknown host/);
  assert.match(err("curl localhost:8087/ping"), /port is 8098/);
});

test("curl defaults", () => {
  assert.equal(ok("curl localhost:8098/ping").method, "GET");
  const post = ok("curl localhost:8098/buckets/b/keys/k -d x");
  assert.equal(post.method, "POST");
  assert.deepEqual(post.headers, [["Content-Type", "application/x-www-form-urlencoded"]]);
  assert.equal(ok("curl -si localhost:8098/ping").include, true);
});

test("bare method form", () => {
  const r = ok("put node2/buckets/b/keys/k hello world");
  assert.equal(r.method, "PUT");
  assert.equal(r.node, 1);
  assert.equal(r.body, "hello world");
  assert.equal(ok("GET /ping").node, null);
});

test("rejects what it does not support", () => {
  assert.match(err("curl -u me:pw localhost:8098/ping"), /option -u is not supported/);
  assert.match(err("curl -d @file.json localhost:8098/buckets/b/keys/k"), /@file/);
  assert.match(err("wget localhost:8098/ping"), /not a command/);
  assert.match(err("curl https://localhost:8098/ping"), /HTTPS/);
});
