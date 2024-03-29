FROM rust as emslibs
# fetch an update version of the link under
# https://www.tibco.com/products/tibco-messaging/downloads
RUN curl -O 'https://edownloads.tibco.com/Installers/tap/EMS-CE/10.2.1/TIB_ems-ce_10.2.1_linux_x86_64.zip?SJCDPTPG=1708493416_524f690c3ea8ee06820607e08db7dfe4&ext=.zip'
RUN unzip TIB*
RUN tar xzf TIB_ems-ce_10.2.1/tar/TIB_ems-ce_10.2.1_linux_x86_64-c_dev_kit.tar.gz
RUN tar xzf TIB_ems-ce_10.2.1/tar/TIB_ems-ce_10.2.1_linux_x86_64-c_dotnet_client.tar.gz
RUN tar xzf TIB_ems-ce_10.2.1/tar/TIB_ems-ce_10.2.1_linux_x86_64-thirdparty.tar.gz

FROM rust as builder
WORKDIR /app
COPY Cargo.toml .
COPY src ./src
COPY --from=emslibs /opt/tibco/ems/10.2/lib/libtibems.so /lib/x86_64-linux-gnu/libtibems.so
COPY --from=emslibs /opt/tibco/ems/10.2/lib/libssl.so.3 /lib/x86_64-linux-gnu/libssl.so.3
COPY --from=emslibs /opt/tibco/ems/10.2/lib/libcrypto.so.3 /lib/x86_64-linux-gnu/libcrypto.so.3
# do a clippy check
RUN rustup component add clippy
RUN cargo clippy --all -- -D warnings
# do release build
RUN cargo install --path .

# Bundle Stage
FROM debian:stable-slim as final
COPY --from=emslibs /opt/tibco/ems/10.2/lib/libtibems.so /lib/x86_64-linux-gnu/libtibems.so
COPY --from=emslibs /opt/tibco/ems/10.2/lib/libssl.so.3 /lib/x86_64-linux-gnu/libssl.so.3
COPY --from=emslibs /opt/tibco/ems/10.2/lib/libcrypto.so.3 /lib/x86_64-linux-gnu/libcrypto.so.3
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
ENV RUST_BACKTRACE=full
ENV RUST_LOG=info
CMD ["./tibco-ems-operator"]
