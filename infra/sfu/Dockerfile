FROM golang:1.26.8-bookworm AS build
RUN git clone https://github.com/livekit/livekit.git /src && cd /src && git checkout 6fbe7f7e648aab4b2f734d6044a416bc2cbc482c
WORKDIR /src
COPY infra/sfu/patch.py /patch.py
COPY infra/sfu/hypercast_quota.go pkg/rtc/hypercast_quota.go
COPY infra/sfu/hypercast_quota_test.go pkg/rtc/hypercast_quota_test.go
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
RUN python3 /patch.py && CGO_ENABLED=0 go test ./pkg/rtc -run TestHypercast && CGO_ENABLED=0 go build -o /livekit-server ./cmd/server
FROM gcr.io/distroless/static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab
LABEL org.opencontainers.image.source="https://github.com/LucasCamy/HyperCast"
COPY --from=build /livekit-server /livekit-server
USER 65532:65532
ENTRYPOINT ["/livekit-server"]
