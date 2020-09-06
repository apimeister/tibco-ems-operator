# FROM gentoo/stage3-amd64 AS builder

# prepare builder
# RUN emerge --sync
# RUN emerge --oneshot sys-apps/portage
# RUN emerge --oneshot sandbox
# RUN emerge rust
FROM rust:1.46.0-alpine AS builder
# RUN apk add --update curl gcc g++ pkgconfig perl make musl musl-dev apk-tools-static
RUN apt-get update && apt-get install -y pkg-config
# WORKDIR /
#RUN ln -s /usr/include/x86_64-linux-gnu/asm /usr/include/x86_64-linux-musl/asm &&     ln -s /usr/include/asm-generic /usr/include/x86_64-linux-musl/asm-generic &&     ln -s /usr/include/linux /usr/include/x86_64-linux-musl/linux
# RUN mkdir /musl
# RUN wget https://github.com/openssl/openssl/archive/OpenSSL_1_1_1g.tar.gz
# RUN tar zxvf OpenSSL_1_1_1g.tar.gz 
# WORKDIR /openssl-OpenSSL_1_1_1g
# RUN CC="musl-gcc -fPIE -pie" ./Configure no-shared no-async --prefix=/musl --openssldir=/musl/ssl linux-x86_64
# RUN make depend
# RUN make -j4
# RUN make install

# ENV PKG_CONFIG_ALLOW_CROSS=1
# ENV OPENSSL_STATIC=true
# ENV OPENSSL_DIR=/musl

WORKDIR /usr/src/
RUN rustup target add x86_64-unknown-linux-musl
# build project
RUN USER=root cargo new tibco-ems-operator
WORKDIR /usr/src/tibco-ems-operator
COPY Cargo.toml .
RUN cargo build --release
COPY src ./src
RUN cargo tree
RUN cargo install --target x86_64-unknown-linux-musl --path .

# Bundle Stage
FROM scratch
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
USER 1000
CMD ["./tibco-ems-operator"]