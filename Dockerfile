FROM rust as emslibs
RUN curl -O 'https://edownloads.tibco.com/Installers/tap/EMS-CE/8.6.0/TIB_ems-ce_8.6.0_linux_x86_64.zip?SJCDPTPG=1619047583_18ff4d1cd9c65dc4e2eefe94d24023e4&ext=.zip'
RUN unzip TIB*
RUN tar xzf TIB_ems-ce_8.6.0/tar/TIB_ems-ce_8.6.0_linux_x86_64-c_dev_kit.tar.gz
RUN tar xzf TIB_ems-ce_8.6.0/tar/TIB_ems-ce_8.6.0_linux_x86_64-c_dotnet_client.tar.gz
RUN tar xzf TIB_ems-ce_8.6.0/tar/TIB_ems-ce_8.6.0_linux_x86_64-thirdparty.tar.gz

FROM rust as builder
WORKDIR /app
ENV EMS_HOME=/opt/tibco/ems/8.6
COPY Cargo.toml .
COPY src ./src
COPY --from=emslibs /opt /opt
RUN cargo install --path .

# Bundle Stage
FROM centos as final
COPY --from=emslibs /opt /opt
ENV LD_LIBRARY_PATH=/opt/tibco/ems/8.6/lib
COPY --from=builder /usr/local/cargo/bin/tibco-ems-operator .
ENV RUST_BACKTRACE=full
CMD ["./tibco-ems-operator"]