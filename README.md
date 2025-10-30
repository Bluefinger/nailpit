# Nailpit

Send Malicious Scrapers into an equally malicious tarpit with added rusty nails. Nailpit is an exercise in offensive security, in which malicious actors (in this case, web scrapers) are targeted and have their resources wasted/attacked. The purpose is to use this against scrapers that *ignore* one's `robots.txt` file and any `Disallow` directives, particularly ones that try to scrape private/non-public sections of one's websites/services. In doing so and with enough volume, such scrapers can constitute an effective DoS attack. *This is bad*. Therefore, this project aims to contribute another tool in making sure such misbehaving scrapers are discouraged from targeting your website by inundating them with garbage and poisoned content.

## Disclaimer

Nailpit is intended to not be exposed to the public, only to bots/scrapers. Any link into the tarpit should be hidden to users, and the initial entry point for Nailpit is a disclaimer as well. This project is not responsible for misconfigured deployments or consequences relating to that. You are responsible for ensuring this is deployed correctly and employed against only agents that are ignoring widely used and accepted web standards such as `robots.txt`.

## Minimum CPU Support

For x86_64/amd64 processors, processors that at least qualify for x86-64-v3 level (so supporting AVX2), and for aarch64, processors from the A53 onwards (with NEON support, so Raspberry PI 3 B+). armv7 and RISCV64 are compiled without instruction optimisations.

## How to use / deploy

By default, `nailpit` won't work unless you provide at least *some* input data. In the directory you are running `nailpit` from, create an `input` directory and add a `.txt` file inside of it. Name it anything, whatever, like `first.txt`. In this file, add in content/text, the more the better, as this will train the markov chain on what to generate. So for example, add many paragraphs of lorem ipsum text to the file just to see it work. Once you have at least *one* txt input, `nailpit` will be able to run. Multiple `.txt` files will act as different markov chains, each one outputting differently structured text to the other, and each time a generated page is requested, these chains are selected at random to produce content for that request. So if you want more varied/randomised content, you want not just very large text files of pure text/content, but also many different files. Do keep in mind that the more content and files you use, the bigger the memory usage of the application, though this is kept in check with some memory optimisation techniques.

The input text files should be pure text. It should not be html or markdown or any other format, just text.

### Docker

The easiest way to run `nailpit` is to run it in a docker container. This makes it fairly easy to deploy and ensures its running environment is consistent. This does add *a bit more* overhead, but realistically, not enough to really matter.

If building the image directly from this repo, just use `docker build . -t nailpit`, or if cross-compiling to a different platform like a Raspberry Pi 5, run `docker build --platform=linux/arm64 .`. `nailpit` docker image supports `linux/amd64`, `linux/arm64`, `linux/arm/v7` and `linux/riscv64` platforms. Running the image then becomes the following (with two volumes provided for user overrides):

```
docker run -v ./configuration/:/app/configuration -v ./input/:/app/input -p 3001:3001/tcp nailpit:latest
```

The socket `nailpit` listens to can be overridden with `-e NAILPIT_SOCKET=0.0.0.0:3001`, and it expects the full ip:port string. There's three volumes to be configured, one for `/app/configuration` which is where the default config file lives and will be where your override config file will live, one for `/app/input` which is where the user's input files are located, and the last `/app/templates` for user provided template overrides.

### Configuration

All of the configuration options are documented in the default config file found [here](./configuration/pit.default.toml). To create your own configuration, create a `pit.toml` file in the configuration folder and add just the configuration options you want to override.

## How to contribute

If you are interested in contributing to this project, check out the [CONTRIBUTING document](CONTRIBUTING.md).

## Code of Conduct

If you are interested in contributing to this project, be sure to review the [CODE OF CONDUCT](CODE_OF_CONDUCT.md).

## License

This project is licensed under AGPL 3.0.
