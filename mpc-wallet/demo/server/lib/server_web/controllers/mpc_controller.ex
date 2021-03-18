defmodule ServerWeb.MPCController do
  use ServerWeb, :controller
  require Poison
  require ConCache

  @doc """
    (re-)fill rpool to facilitate signature completion with a single message.

    Input: client_dh_public_keys (list of client DH public keys)
    Output: server_dh_public keys (list of server DH public keys)

    The server must ensure that size of rpool is bounded. Currently set to 100 in the client. see fill_rpool() in mpc-wallet-lib/src/client.rs
  """
  def dh_rpool(conn, _params) do
    client_dh_publics = conn.body_params["client_dh_publics"]
    curve = conn.body_params["curve"]
    {:ok, rpool_new, server_dh_publics} = Server.MPCwallet.dh_rpool(client_dh_publics, curve)
    rpool_new = Poison.decode!(rpool_new)
    # add to existing pool (exclusively accessing that data!)
    ConCache.isolated(:demo_cache, :rpool, nil, fn() ->
      rpool_old = ConCache.get(:demo_cache, :rpool)
      if !is_nil(rpool_old) do
        # merge old and new rpools
        rpool = Map.merge(rpool_new, rpool_old)
        ConCache.put(:demo_cache, :rpool, rpool)
      else
        ConCache.put(:demo_cache, :rpool, rpool_new)
      end
    end)

    json(conn, server_dh_publics)
  end

  @doc """
    Complete ECDSA signature from presignature.

    Input:
      r (message-independent part of the signature),
      presig (presignature from client),
      curve (Secp256k1 or Secp256r1),
      hash of the message under which the completed signature is (supposed to be) valid. This is only for demo-purposes. In a real-world scenario, the server is supposed to compute the hash itself.

    Output:
      r: r part of a conventional ECDSA signature
      s: s part of a conventional ECDSA signature
      recovery_id: 2 bits that allow recovering the public key from a signature

    Needs to fetch paillier_sk from session cache (should actually be some persistent storage).
    Needs to fetch and delete k_inv atomically(!) from session cache.
    Needs to fetch the public key that corresponds to the user's API child key from session cache (should actually be some persistent storage).

    Depending on the signing policy, the server may complete the signature. This is not implemented in this demo.
  """
  def complete_sig_ecdsa(conn, _params) do
    r = conn.body_params["r"]
    presig = conn.body_params["presig"]
    curve = conn.body_params["curve"]
    pubkey = ConCache.get(:demo_cache, :pubkey)
    msg_hash = conn.body_params["msg_hash"]
    paillier_sk = ConCache.get(:demo_cache, :paillier_sk)

    # make sure that the r value is used exclusively for a particular message (even when this function is called concurrently)
    # ensure that a value is consumed exactly once by mutual exclusive access to session cache. this is security-critical!
    k_inv = ConCache.isolated(:demo_cache, :rpool, nil, fn() ->
      rpool = ConCache.get(:demo_cache, :rpool)
      # fetch k_inv from pool
      k_inv = rpool[r]
      # delete r-value from pool and write updated rpool to session cache
      rpool = Map.delete(rpool, r)
      ConCache.put(:demo_cache, :rpool, rpool)
      k_inv
    end)

    {:ok, r, s, recid} = Server.MPCwallet.complete_sig_ecdsa(paillier_sk, presig, r, k_inv, curve, pubkey, msg_hash)

    out = %{
      r: r,
      s: s,
      recovery_id: recid,
    }
    json(conn, out)
  end

  @doc """
    Complete EdDSA signature from presignature.

    Input:
      r (message-independent part of the signature),
      presig (presignature from client),
      message

    Output:
      r: r part of an EdDSA signature
      s: s part of an EdDSA signature

    Needs to fetch and delete k_inv atomically(!) from session cache.
    Needs to fetch the public key that corresponds to the user's API child key from session cache (should actually be some persistent storage).

    Depending on the signing policy, the server may complete the signature. This is not implemented in this demo.
  """
  def complete_sig_eddsa(conn, _params) do
    r = conn.body_params["r"]
    presig = conn.body_params["presig"]
    msg = conn.body_params["msg"]
    pubkey = ConCache.get(:demo_cache, :pubkey)
    server_secret_share = ConCache.get(:demo_cache, :server_secret_share)

    # make sure that the r value is used exclusively for a particular message (even when this function is called concurrently)
    # ensure that a value is consumed exactly once by mutual exclusive access to session cache. this is security-critical!
    r_server = ConCache.isolated(:demo_cache, :rpool, nil, fn() ->
      rpool = ConCache.get(:demo_cache, :rpool)
      # fetch r_server from pool
      r_server = rpool[r]
      # delete r-value from pool and write updated rpool to session cache
      rpool = Map.delete(rpool, r)
      ConCache.put(:demo_cache, :rpool, rpool)
      r_server
    end)

    {:ok, r, s} = Server.MPCwallet.complete_sig_eddsa(server_secret_share, presig, r, r_server, pubkey, msg)

    out = %{
      r: r,
      s: s,
    }
    json(conn, out)
  end

  @doc """
    Get paillier keypair and proof that the paillier keypair was generated correctly.

    Input: None

    Output:
      correct_key_proof: correct key proof
      paillier_pk: Paillier public key
  """
  def get_paillier_keypair_and_proof(conn, _params) do
    # paillier keypair and proof generated by Server.MPCwallet.generate_paillier_keypair_and_proof()
    # same paillier key pair (and proof) is used for all users
    # For any real-world implementation, the key pair and proof should not be stored in the code.
    paillier_sk = "{\"p\":\"92bbf210646f235218a3107eda9f8a30c6e76b52dcb7557f46941a1c1c3e9534b99e9bd3bd774286244bd39053ef1c9031041517c1e9cebfe4a3b402b5f18a1be30beb15a637cf735148d32b1089c37faccd0ae9a7a17dcdf92c7691cbe1a17c2053e584db6286b853b774834a30a2c77d1618c57551341573cfbee6f3784167\",\"q\":\"d25da670218064b059a7866b7fde5ff516244ca2ec4a4038d0cef3ddae6310936a384d01dc81da0130e538794c24b98c6ad7c10bcc79cfa27b4d0fb280741c16f35665c153191e2c36dffe65eb269dc5ffa868c92ab80e1469dc6857b65d84af09669ed91a7c79b30343fd12cc8ede8c2f68175c04ee0342d708c417cae5cfcf\"}"
    paillier_pk = "{\"n\":\"7893da3e86bc0188d2dc2117e00d11938df061f4e94b0ce7ec6f5e72f6fb25699de56fd536457502a40691d2524c41bd69c9eb73d80c197b5815babab8acc596b57b503409f83ade97a9861670ca3e8af232350344dabf820f2b1c4f152a27c6041ec3dbb0ddc1d221084070c45b11c22055bd669a09d88fa6f3352b8778bde362aa01950920dc37e61e19571009ead6cd848b8cdaed792e295f12ff9c395c509e51dd180e11b8a8ad4fa56355649c6b364bed04136a8d70109af77af01f4d1caf0ffbafa51b1694d250c13bf8be4817d7a5caeed1bd6596301250efafcf0ad296987f0834a6031f4461c1ef14fe8cac7e74d1e90564a0db1f25860be2422b49\"}"
    correct_key_proof = "{\"sigma_vec\":[\"76a0c95092dc40b4fa381694b0eed82be0c41346010ebf7df8d0f67a66e05f9abe6f7dc680b7f06292ded03cdc9d15badfc7ed61884c211903b44641f5a35f1441fba83aa280695ab252c4b90c60f997279ab8cfb9e32e0cfc327d44a04b4516d05f372d9fe96956e44ecc8c0379f2e7a6e519fbb9925744a1fd8a1b0852cb5d2e774bdbf3a239eb8c783b11707ec63f29b3f43e8028797bf243b12c8160e1d2d57deafc197fcb87f07b24f0c52528e2320d4777c199ffeac27328b80f548c5326285a125f142ecbeb5a6c18af96ca58a04d7cb36f54eea57ffc9ac3c0f247e54102a998b61c70a155c07bcddcac5f6d402900f6aa54837d6c834a44b840d025\",\"0b07c20e5289679df1862612c0f070283df96e3bb5551f3399fc4e43c0eaf39edf5511e6d007cb3e3bf63deed51bdee0acb8519f817ced885bc8406ca459ca2d11578dce5a41e6f19bc2fbfb89feb82d8b8cba6f1bd94e0e985617da1023b761db7c2b12ba2d8f594326d11fcbafdec2c1062701076618332493902cc2e261bfb139a5fc154280b3fcb768282960508bdeceea3a6c85855b428d18f3013564be117228ffe1f028bb7bad5a8dc6005c246ca0e227036c8521cc1b38bc350388812b829b58e2d2e38061052292ec1b4763c77b1249be49ba830fe8a20ef97063e11bd423f86fbac317a2252b01b07099e0bc3417e3953c05a916eeecdc35017f32\",\"6d5df38629fc524fdb8f555db15d0916053caa77ba55af176a609d8d987df599c60ee46fdc7ba6368485681f14b99c64e3d6fa9f2873c83ff4f4b63f49920e98f038eb1dbf0ec758c9055f6da75b43614f69898a53143c73d65deeb31914c349cf97b4b5ee4863f06a1cc237957ef96193585e2b768f5bfa06fa952adbf33f1ea6afeb071e1788319b9e89a948cd6e9ccfabf0c2b1aec340977f0eaba947120da055e385a4ef87ec29897e697be270675d0e3bb3ac99115b2687af7e8a9fda15413b6e457b3e3ed73deae930ce62390c5c01d0a415b18bb54db5052fba61452442c314f08412f7b53d9bed292b4acd292ceff6daf61c272fa706d359f39a4b0f\",\"44aa56fd1e0a25e4de100956059dd6bd078a644aa33d8ea13a6f00cbf4e2fa33ba30fac2540ed7122b734689c171461c6742257d51c5f0a174bd29185f0fa430b7430f5e9c9557facc527c416e8a4b38afcedc69ab8429c10fe5307e0a7d61519f44ed44ce1fb9ebc368b0282707bb8e59311a0e8ee1b9722c403d90a2e21eb92f80f0b5508d28660367b27664683568a05ebbde2a044cb312d5781eb2c85264ff8154a1e603fe26767063b4b673f82b8859d93fce6c20ab079eb1f2e48b079f321776a5aa53f8ee6d06da0b08a7715c6dee0ae49dd741c73b427cc70c93347e334fde1e4830b625aea2fda0db7cf7e2176a194f10c3b6e8d4ecd0dca797d6a9\",\"52c3f9b660dc389ae83b78a117c6d52281bc5c797fdfad4fb2a6c63dfbe7394320a912ea0f362283289e098c11a25e220e33fa87e54e2c1176633a46dd3a5e65815fda09e7a903880b61e0a2b4827aba5be34210ae604645d8b9a6cdfa9d35935c2611b2e2b55a04559d4252d571745f1f7f1d4be380e0e9cb61061993c451b7ebefb251963267d2076ad3f197ceee8cd0149fd3e4a734ca9492dda3b9d38b258dcdedec57c7fe33d5a81b136cf387b7bee7716510822ff1cf39d04b75d2b00b0bcc671ca9f5bae4911788989aafc33661fcf4ffc489c68e7591c65fb0fa7ddfc782dbd9f65479c817f57a067e8884e345ecc65e409c0e22f0fe6bf6be7fe664\",\"436c227189148a9a0243017372ee7eb171ec8175933492d7081b31591bf95d174b2c2aefc8d073ea3450246a2d0554b0db105211573e31e63aabe10d5fb53dd2b50e662295b5ac9bf3d6e9b43abdd1e14763328edec782c92a381c5760ff344ee029f3a8360c039b87d9a2c50147d80d5996999fb2ae14255165ef6640e597752aeaa36bee94d92a846b8806f023526cce0e81150de81af04fcdf25c3635a307e243ac741b441f6894e656266b0422d750dca5cb096dc1a39609189538ba6165a7cbb1a0a69c2c1ffd0a8f86f60d2cf8a42e069965f778d75c242e010fdcd5a6bb9f1e58397daee39d0b862d255c10a3fc64a585be9ed9682c99390faf818f84\",\"65150a3b42417069387c28290a84ae62703210c480a6cd0c9e4fdeeb73493c1a559b1acb9b768756b0ffc9c9777145edbcac3731b1e18e0c804945b83f3e45416f3d4b433c1b79e34437733f34d9cfdaaa4298786e11d7daa3c954bd55390dee5a1448666f806b773ad1ef83076dad3e1645f51ee4d9317f57f6db26ae9f8561b872116c3cd0320d61dda5380fa69ba2470694d8377c8d2dd7ec1fee17446733f0e4b368e72f3b10cdfaa31a7abdfc29a0ad7053bb14a1b9621da4b42a4d08e8586afb3751c7cedc9c31491e692b48a3f07f430f0e4ba4b7fdeb91481b1271251c6acab47371dc3945f169633a27237085a339033676627d0536de57a750e242\",\"4b5c9c5379f6c21d7c60c2a5d47811bffec3c0dcbc1342d41741e54203e44e039d1a2bdb23806cee42a480b049468146204fa1562b9d4096a7f1cce93c094c37c388fc04da58acceeae503719eccfe7565226edc1a4059584dfc5b0523b6b25f47642142d3ea01f51d9029368210d5202da7d64c4790d558887b0abebc95d0ed87a59d8141c1a9ba38bfe05b397463018173bbb6e1f8bab444d3ea66d039ebce2095fd5904f88bc853b6b773a50eaa2bf23f8062cfdac157ee99deaac3eef48114a7cda0edc62f4aa56c05de05aeac26d56dcdb05e424d52fa335d320a3cc28041cc2dfa9e48e6b66b806d74f31614dc6f7d677eb12a9853201d6328cacf14fd\",\"5e8bd15ae70e3ffe80f0667378f1794d92943535c56b498841377d14ecf89c44ec6b244d943801fd137eeb0cdbe31fe6b576c34bc89f8daec843ef91f17380c1d76d8d77fae9fb6e417db93017c89fe9f08c59bf3d93610263d04fb4510d5572c0cda9d137b9dc463f04f18ac8c0156dc3e98ccb6694c5d2a1dad7d1fdc464b33f1ae40afdd128087269278b9aa5453d7708b7969c566bb3d49854b42f6eb5b8a4997cee97ec153f61d54d2b7ea71a0980d2a3880f9c9cd00617de7f29148b82a072360abdd2b320a21f9a5de426d8437e344d905e64fb25b1b2870e7b016b764081932c01da34f96320f7d60658f2a0ab0c77cd73d5c1222c2486d09a5d9a02\",\"0e1885487c168981a347b2e577a47b2c8517768cf5c6686f896e4a36013b0bf3e78af5c829943e86bc7c1a72cc1cf3051099dff87a1bac64f45bc738c2b21063b95360c4f7f0f0046f4e7e3052d9e7800873e8963501081e90996e4e3fc7558842939e2aec36a0cb8b6d48bf3d24d9e500c4ca84fff9e2ec6f034a7fb68f1b8a754ee69e474aa156cf40243a59d060dd55e040d94e569d24939c2a1336406cf18d305352945294b941aaa52effa5481249d86517f26eb60a1ba645fe451730b01764a44df0f7c7d5c2426516b04381e328bc40634c7a3c0d2a7befb14fb8c754d70b422b2be252ec4af0735597300620ff0153d19caf73f8223683a9dd679c45\",\"214019f1cc5028534287e83c770d006af7734e502199a79f67567d5e8538abe62eaf6c5fe03c3125b25da1217791a9188efb5576b2eb747b45b3b7a48ee894220196304944e91054f3549ba5c34cc7a391f40ee0abe59dc93eb8bb2e8a49a6fd76c7b855854a9a800885e0369abc89dc0d1070f181dee95240bc579cf59a85ecea8fd13f8ba029c277680f9c5deefd6cc76e9b60b349a4028e8f988aa39418e0d2bc4384a7cc653475c3e903162d212dd1c63f230e459bb3add85386bb4d01aa71748f2484f677e6c3d53d75549c0ffcf0ae877420096d6516be3ec67bec45c77f7df18aa92967b8cf0d32eec0ac8fda5d46981fb2a833afeb45b3674d0df14e\"]}"
    ConCache.put(:demo_cache, :paillier_sk, paillier_sk)
    out = %{
      correct_key_proof: Poison.decode!(correct_key_proof),
      paillier_pk: Poison.decode!(paillier_pk),
    }
    json(conn, out)
  end

  @doc """
    Register API child key with the server.

    Input:
      corresponding public key
      encrypted server secret share (only for EdDSA/Ed25519)

    Output:
      None

    The registered information needs to be stored per user account.
    Needs to fetch paillier_sk from session cache (should actually be some persistent storage).
  """
  def register_apikey(conn, _params) do
    pubkey = conn.body_params["pubkey"]
    server_secret_share_encrypted = conn.body_params["server_secret_share_encrypted"]
    ConCache.put(:demo_cache, :pubkey, pubkey)
    if !is_nil(server_secret_share_encrypted) do
      paillier_sk = ConCache.get(:demo_cache, :paillier_sk)
      {:ok, server_secret_share} = Server.MPCwallet.decrypt(paillier_sk, server_secret_share_encrypted)
      ConCache.put(:demo_cache, :server_secret_share, Poison.decode!(server_secret_share))
    end
    json(conn, %{})
  end
end
