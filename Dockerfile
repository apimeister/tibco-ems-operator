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
# FROM alpine
FROM centos
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
COPY bin/libtibemsadmin64.so bin/libtibems64.so /usr/lib

CMD ["./tibco-ems-operator"]