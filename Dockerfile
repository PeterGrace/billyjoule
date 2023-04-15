FROM docker.io/ubuntu:22.04
ARG TARGETARCH

RUN mkdir -p /opt/billyjoule
WORKDIR /opt/billyjoule
COPY ./tools/target_arch.sh /opt/billyjoule
RUN --mount=type=bind,target=/context \
 cp /context/target/$(/opt/tolerable/target_arch.sh)/release/billyjoule /opt/billyjoule/billyjoule
CMD ["/opt/billyjoule/billyjoule"]
EXPOSE 9090
