FROM rust as planner
WORKDIR /app
RUN cargo install cargo-chef 
COPY Cargo.toml .
COPY src ./src
COPY deps ./deps
COPY build.rs .
RUN cargo chef prepare  --recipe-path recipe.json

FROM rust as cacher
WORKDIR /app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust as builder
WORKDIR /app
COPY Cargo.toml .
COPY src ./src
COPY deps ./deps
COPY build.rs .
COPY --from=cacher /app/target target
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