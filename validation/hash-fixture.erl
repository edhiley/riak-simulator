%% Evaluated inside a real OpenRiak node (riak eval) to produce
%% core/tests/fixtures/hash-real-3.4.1.tsv. See validation/make-hash-fixture.sh.
Hex = fun(B) -> binary:encode_hex(B, lowercase) end,
Succ = fun(HB, Size) ->
    Ring = chash:fresh(Size, n),
    Ps = [integer_to_list(I) || {I, _} <- lists:sublist(chash:successors(HB, Ring), 3)],
    string:join(Ps, ",")
end,
Keys = [<<"k">>, <<>>, <<"a b">>, <<"ünïcödé"/utf8>>, <<0, 255, 1>>, binary:copy(<<"x">>, 300)]
    ++ [list_to_binary("key-" ++ integer_to_list(N)) || N <- lists:seq(1, 300)],
Buckets = [<<"b">>, {<<"plain">>, <<"b">>}, {<<"default">>, <<"b">>}, <<"users">>, {<<"maps">>, <<"carts">>}],
Lines = [begin
    {T, B} = case Bk of {Ty, Bu} -> {Ty, Bu}; Bu -> {<<>>, Bu} end,
    HB = riak_core_util:chash_key({Bk, K}, riak_core_bucket:get_bucket(Bk)),
    io_lib:format("~s\t~s\t~s\t~s\t~s\t~s~n", [Hex(T), Hex(B), Hex(K), Hex(HB), Succ(HB, 8), Succ(HB, 64)])
  end || Bk <- Buckets, K <- Keys],
file:write_file("/tmp/hash.tsv", ["# type_hex\tbucket_hex\tkey_hex\tsha1_hex\tpreflist_ring8\tpreflist_ring64\n" | Lines]).
