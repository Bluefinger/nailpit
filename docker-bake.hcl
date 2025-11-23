target "default" {
  context = "."
  dockerfile = "Dockerfile"
  tags = ["docker.io/sachymetsu/nailpit:latest"]
  no-cache = true
  platforms = ["linux/amd64", "linux/arm64"]
}
