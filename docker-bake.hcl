target "default" {
  context = "."
  dockerfile = "Dockerfile"
  tags = ["docker.io/sachymetsu/nailpit:latest"]
  platforms = ["linux/amd64", "linux/arm64"]
}
