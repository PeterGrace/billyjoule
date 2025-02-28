FROM docker.io/ubuntu:24.04
ARG TARGETARCH
ARG GUILD_ID
ARG CHANNEL_ID

RUN mkdir -p /opt/billyjoule
WORKDIR /opt/billyjoule
COPY ./tools/target_arch.sh /opt/billyjoule
RUN --mount=type=bind,target=/context \
 cp /context/target/$(/opt/billyjoule/target_arch.sh)/release/billyjoule /opt/billyjoule/billyjoule
CMD ["/opt/billyjoule/billyjoule", "--guild-id", "$GUILD_ID", "--channel-id", "$CHANNEL_ID"]
EXPOSE 9090
