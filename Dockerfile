FROM rust as emslibs
# fetch an update version of the link under
# https://www.tibco.com/products/tibco-messaging/downloads
RUN curl -O 'https://edownloads.tibco.com/Installers/tap/EMS-CE/8.6.0/TIB_ems-ce_8.6.0_linux_x86_64.zip?SJCDPTPG=1629572437_2acfe94c089683e225bc46f78ff84fcd&ext=.zip'
RUN unzip TIB*
RUN tar xzf TIB_ems-ce_8.6.0/tar/TIB_ems-ce_8.6.0_linux_x86_64-c_dev_kit.tar.gz
RUN tar xzf TIB_ems-ce_8.6.0/tar/TIB_ems-ce_8.6.0_linux_x86_64-c_dotnet_client.tar.gz
RUN tar xzf TIB_ems-ce_8.6.0/tar/TIB_ems-ce_8.6.0_linux_x86_64-thirdparty.tar.gz

FROM rust as builder
WORKDIR /app
COPY Cargo.toml .
COPY src ./src
COPY --from=emslibs /opt/tibco/ems/8.6/lib/libtibems.so /lib64/libtibems.so
RUN cargo install --path .

# Bundle Stage
FROM rockylinux/rockylinux as final
RUN dnf -y update && dnf clean all
COPY --from=emslibs /opt/tibco/ems/8.6/lib/libtibems.so /lib64/libtibems.so
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
ENV RUST_BACKTRACE=full
CMD ["./tibco-ems-operator"]