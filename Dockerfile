FROM rust as emslibs
RUN curl -O 'https://edownloads.tibco.com/Installers/tap/EMS-CE/8.5.1/TIB_ems-ce_8.5.1_linux_x86_64.zip?SJCDPTPG=1612501173_69dc25fd9c51e5b68fd4e2b11ee6cac0&ext=.zip'
RUN unzip TIB*
RUN tar xzf TIB_ems-ce_8.5.1/tar/TIB_ems-ce_8.5.1_linux_x86_64-c_dev_kit.tar.gz
RUN tar xzf TIB_ems-ce_8.5.1/tar/TIB_ems-ce_8.5.1_linux_x86_64-c_dotnet_client.tar.gz
RUN tar xzf TIB_ems-ce_8.5.1/tar/TIB_ems-ce_8.5.1_linux_x86_64-thirdparty.tar.gz

FROM rust as planner
WORKDIR /app
RUN cargo install cargo-chef 
COPY Cargo.toml .
COPY src ./src
RUN cargo chef prepare  --recipe-path recipe.json

FROM rust as cacher
WORKDIR /app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust as builder
WORKDIR /app
ENV EMS_HOME=/opt/tibco/ems/8.5
COPY Cargo.toml .
COPY src ./src
COPY --from=emslibs /opt /opt
COPY --from=cacher /app/target target
RUN cargo install --path .

# Bundle Stage
FROM centos as final
COPY --from=emslibs /opt /opt
ENV LD_LIBRARY_PATH=/opt/tibco/ems/8.5/lib:/opt/tibco/ems/8.5/lib/64
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
ENV RUST_BACKTRACE=full
CMD ["./tibco-ems-operator"]