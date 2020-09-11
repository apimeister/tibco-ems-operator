FROM rust:1.46.0-alpine AS builder
# prepare builder
RUN apk add --update gcc g++ pkgconfig openssl-dev

# build project
WORKDIR /usr/src/
RUN USER=root cargo new tibco-ems-operator
WORKDIR /usr/src/tibco-ems-operator
COPY Cargo.toml .
COPY src ./src
RUN cargo install --path .
# RUN cargo build

# CMD ["/bin/sh"]


# Bundle Stage
FROM centos as final
RUN ln -s /usr/lib64 /usr/lib/64
RUN cp /usr/lib64/libz.so.1 /usr/lib64/libz.so
RUN cp /usr/lib64/libcrypto.so.1.1 /usr/lib64/libcrypto.so
RUN cp /usr/lib64/libssl.so.1.1 /usr/lib64/libssl.so
COPY bin/tibemsadmin/bin/tibemsadmin /usr/bin
COPY bin/tibemsadmin/lib/libtibemsadmin64.so bin/tibemsadmin/lib/libtibems64.so /usr/lib/
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
CMD ["./tibco-ems-operator"]