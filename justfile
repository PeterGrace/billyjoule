commit := `git rev-parse HEAD`
shortcommit := `git rev-parse --short HEAD`
tag := `git tag --points-at HEAD`
transport := "docker://"
registry := "docker.io"
image := "petergrace/billyjoule"

build-aarch64:
  cross build --target aarch64-unknown-linux-gnu --release

build-x86_64:
  cross build --target x86_64-unknown-linux-gnu --release

build: build-aarch64 build-x86_64

image:
  docker buildx build --no-cache --push --platform linux/amd64,linux/arm64/v8 \
  -t {{registry}}/{{image}}:latest \
  -t {{registry}}/{{image}}:{{shortcommit}} \
  -t {{registry}}/{{image}}:{{commit}} \
  .

release-patch:
  cargo release --no-publish --no-verify patch --execute
release-minor:
  cargo release --no-publish --no-verify minor --execute
release-major:
  cargo release --no-publish --no-verify major --execute
