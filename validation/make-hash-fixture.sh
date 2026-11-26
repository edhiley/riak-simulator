#!/bin/sh
# Regenerate core/tests/fixtures/hash-real-3.4.1.tsv from a real OpenRiak node,
# then check the node's Protocol Buffers preflist call agrees with it.
# Needs the real-riak:3.4.1 image from ../riak-emu/validation/real-riak.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
fixture="$here/../core/tests/fixtures/hash-real-3.4.1.tsv"
trap 'docker rm -f riak-sim-real >/dev/null 2>&1 || true' EXIT
docker rm -f riak-sim-real >/dev/null 2>&1 || true
docker run -d --name riak-sim-real -p 28087:8087 real-riak:3.4.1 >/dev/null
until docker logs riak-sim-real 2>&1 | grep -q "riak ready"; do sleep 2; done
riak=/home/riak/riak/rel/riak/bin/riak
docker exec riak-sim-real "$riak" eval "$(grep -v '^%%' "$here/hash-fixture.erl" | tr '\n' ' ')"
docker cp riak-sim-real:/tmp/hash.tsv "$fixture"
python3 "$here/pb-preflist.py" localhost:28087 "$fixture"
