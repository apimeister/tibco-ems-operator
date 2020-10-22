FROM rust:1 AS builder
# prepare builder
# RUN yum -y install cargo krb5-libs
# build project
WORKDIR /usr/src/
RUN USER=root cargo new tibco-ems-operator
WORKDIR /usr/src/tibco-ems-operator
COPY Cargo.toml .
COPY src ./src
COPY deps ./deps
COPY build.rs .
RUN cargo install --path .

# Bundle Stage
FROM centos as final
RUN ln -s /usr/lib64/libz.so.1 /usr/lib64/libz.so
RUN ln -s /usr/lib64/libcrypto.so.1.1 /usr/lib64/libcrypto.so
RUN ln -s /usr/lib64/libssl.so.1.1 /usr/lib64/libssl.so
COPY deps/tibemsadmin* /usr/bin
COPY deps/*.so /usr/lib64/
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
ENV RUST_BACKTRACE=full
CMD ["./tibco-ems-operator"]