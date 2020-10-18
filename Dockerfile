FROM rust:1-alpine AS builder
# prepare builder
RUN apk add --update gcc g++
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
COPY private.key .
COPY cert.crt .
ENV RUST_BACKTRACE=full
CMD ["./tibco-ems-operator"]