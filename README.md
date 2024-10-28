# Project Overview
A terminal editor written in rust.

## SUPER DUPER EARLY ALPHA
This project is super super early... we literally dont have a editor yet, just a bunch of foundations%s!

# Dependencies
You only require [Earthly](https://earthly.dev/) to build the project and it will build the binary in a docker container for you then copy it to the host.

# Run
To get started, clone the repository

You can remove `--release=false` if you want a more performant build, but it takes much longer to build.

## Run in docker
This builds and runs the app in a docker container for you to interact with.
```bash
earthly +run --release=false
```

## Copy to host
This builds the project and copies it to `./artifacts/arcane`
```bash
earthly +build --release=false
```
