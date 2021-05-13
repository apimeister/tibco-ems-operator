FROM rust as emslibs
RUN curl -O 'https://edownloads.tibco.com/Installers/tap/EMS-CE/8.6.0/TIB_ems-ce_8.6.0_linux_x86_64.zip?SJCDPTPG=1623514729_9ebcb6cb70effb4199d7a290e656ba18&ext=.zip'
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
FROM centos as final
RUN dnf -y update && dnf clean all
COPY --from=emslibs /opt/tibco/ems/8.6/lib/libtibems.so /lib64/libtibems.so
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
ENV RUST_BACKTRACE=full
CMD ["./tibco-ems-operator"]