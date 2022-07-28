FROM rust as emslibs
# fetch an update version of the link under
# https://www.tibco.com/products/tibco-messaging/downloads
RUN curl -O 'https://edownloads.tibco.com/Installers/tap/EMS-CE/10.1.0/TIB_ems-ce_10.1.0_linux_x86_64.zip?SJCDPTPG=1661559829_550d23e46bbb9e77f41490c38569103e&ext=.zip'
RUN unzip TIB*
RUN tar xzf TIB_ems-ce_10.1.0/tar/TIB_ems-ce_10.1.0_linux_x86_64-c_dev_kit.tar.gz
RUN tar xzf TIB_ems-ce_10.1.0/tar/TIB_ems-ce_10.1.0_linux_x86_64-c_dotnet_client.tar.gz
RUN tar xzf TIB_ems-ce_10.1.0/tar/TIB_ems-ce_10.1.0_linux_x86_64-thirdparty.tar.gz

FROM rust as builder
WORKDIR /app
COPY Cargo.toml .
COPY src ./src
COPY --from=emslibs /opt/tibco/ems/10.1/lib/libtibems.so /lib/x86_64-linux-gnu/libtibems.so
# do a clippy check
RUN rustup component add clippy
RUN cargo clippy --all -- -D warnings
# do release build
RUN cargo install --path .

# Bundle Stage
FROM debian:stable-slim as final
COPY --from=emslibs /opt/tibco/ems/10.1/lib/libtibems.so /lib/x86_64-linux-gnu/libtibems.so
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
ENV RUST_BACKTRACE=full
ENV RUST_LOG=info
CMD ["./tibco-ems-operator"]