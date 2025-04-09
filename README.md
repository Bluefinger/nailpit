# Nailpit

Send Malicious Scrapers into an equally malicious tarpit with added rusty nails. Nailpit is an exercise in offensive security, in which malicious actors (in this case, web scrapers) are targeted and have their resources wasted/attacked. The purpose is to use this against scrapers that *ignore* one's `robots.txt` file and any `Disallow` directives, particularly ones that try to scrape private/non-public sections of one's websites/services. In doing so and with enough volume, such scrapers can constitute an effective DoS attack. *This is bad*. Therefore, this project aims to contribute another tool in making sure such misbehaving scrapers are discouraged from targeting your website by inundating them with garbage and poisoned content.

## Disclaimer

Nailpit is intended to not be exposed to the public, only to bots/scrapers. Any link into the tarpit should be hidden to users, and the initial entry point for Nailpit is a disclaimer as well. This project is not responsible for misconfigured deployments or consequences relating to that. You are responsible for ensuring this is deployed correctly and employed against only agents that are ignoring widely used and accepted web standards such as `robots.txt`.

## How to use / deploy

TBA/WIP

## How to contribute

If you are interested in contributing to this project, check out the [CONTRIBUTING document](CONTRIBUTING.md).

## Code of Conduct

If you are interested in contributing to this project, be sure to review the [CODE OF CONDUCT](CODE_OF_CONDUCT.md).

## License

This project is licensed under AGPL 3.0.
