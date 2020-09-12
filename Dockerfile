FROM rust:1.46.0-alpine AS builder
# prepare builder
RUN apk add --update gcc g++ pkgconfig openssl-dev
RUN wget -O sccache.tar.gz 'https://github.com/mozilla/sccache/releases/download/0.2.13/sccache-0.2.13-x86_64-unknown-linux-musl.tar.gz' \
    && tar xzf sccache.tar.gz \
    && cp /sccache-0.2.13-x86_64-unknown-linux-musl/sccache /usr/bin/sccache \
    && rm sccache.tar.gz \
    && rm -rf /sccache-0.2.13-x86_64-unknown-linux-musl
ENV RUSTC_WRAPPER=/usr/bin/sccache
ENV SCCACHE_REDIS=redis://MacBook-Pro.fritz.box:6379/

# build project
WORKDIR /usr/src/
RUN USER=root cargo new tibco-ems-operator
WORKDIR /usr/src/tibco-ems-operator
COPY Cargo.toml .
COPY src ./src
RUN cargo install --path .

# Bundle Stage
FROM centos as final
RUN ln -s /usr/lib64/libz.so.1 /usr/lib64/libz.so
RUN ln -s /usr/lib64/libcrypto.so.1.1 /usr/lib64/libcrypto.so
RUN ln -s /usr/lib64/libssl.so.1.1 /usr/lib64/libssl.so
COPY bin/tibemsadmin* /usr/bin
COPY bin/*.so /usr/lib64/
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
CMD ["./tibco-ems-operator"]