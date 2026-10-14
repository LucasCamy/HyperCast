FROM python:3.13-slim-bookworm@sha256:a1165e272e578941b84abc79e4ab38a0305cd12803a5c4247979ac7655f4d641
WORKDIR /app
COPY infra/auth_service.py infra/room_creation.py /app/infra/
COPY tools/room_token.py /app/tools/room_token.py
USER 65532:65532
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
CMD ["python", "-m", "infra.auth_service"]
