FROM trzeci/emscripten-slim:latest

RUN apt-get update \
  && apt-get install -y \
  autoconf \
  libtool \
  build-essential

COPY secp256k1 /secp256k1

WORKDIR /secp256k1

RUN ./autogen.sh
RUN emconfigure ./configure --enable-module-recovery \
  # uncomment next line for debug build:
  # CFLAGS="-g -O0"
  # uncomment next line for production build:
  CFLAGS="-Os"
RUN emmake make FORMAT=wasm
RUN mkdir -p out/secp256k1

RUN emcc src/libsecp256k1_la-secp256k1.o \
  # uncomment next line for debug build:
  # -O0 -g4 -s ASSERTIONS=2 --source-map-base ../../../wasm/secp256k1 \
  # uncomment next line for production build:
  -O3 \
  -s WASM=1 \
  -s "BINARYEN_METHOD='native-wasm'" \
  -s NO_EXIT_RUNTIME=1 \
  -s DETERMINISTIC=1 \
  -s EXPORTED_FUNCTIONS='[ \
  "_malloc", \
  "_free", \
  "_secp256k1_context_create", \
  "_secp256k1_context_randomize", \
  "_secp256k1_ec_seckey_verify", \
  "_secp256k1_ec_privkey_tweak_add", \
  "_secp256k1_ec_privkey_tweak_mul", \
  "_secp256k1_ec_pubkey_create", \
  "_secp256k1_ec_pubkey_parse", \
  "_secp256k1_ec_pubkey_serialize", \
  "_secp256k1_ec_pubkey_tweak_add", \
  "_secp256k1_ec_pubkey_tweak_mul", \
  "_secp256k1_ecdsa_recover", \
  "_secp256k1_ecdsa_recoverable_signature_serialize_compact", \
  "_secp256k1_ecdsa_recoverable_signature_parse_compact", \
  "_secp256k1_ecdsa_sign", \
  "_secp256k1_ecdsa_signature_normalize", \
  "_secp256k1_ecdsa_signature_parse_der", \
  "_secp256k1_ecdsa_signature_parse_compact", \
  "_secp256k1_ecdsa_signature_serialize_der", \
  "_secp256k1_ecdsa_signature_serialize_compact", \
  "_secp256k1_ecdsa_sign_recoverable", \
  "_secp256k1_ecdsa_verify" \
  ]' \
  -o out/secp256k1/secp256k1.js

RUN cp -r /secp256k1/out /secp-wasm-out

# copy outputs to mounted volume
CMD ["cp", "-r", "/secp-wasm-out", "/out"]
