# Project Overview
A terminal editor written in rust.

## SUPER DUPER EARLY ALPHA
This project is super super early... we literally dont have a editor yet, just a bunch of foundations!

# Dependencies
You only require [Earthly](https://earthly.dev/get-earthly) to build the project and it will build the binary in a docker container for you then copy it to the host. You can also compile it directly using rust if you have it installed, using `cargo run` / `cargo build`, the instructions below are for earthly

# Run
To get started, clone the repository

You can remove `--release=false` if you want a more performant build, but it takes much longer to build.


## Run in docker
This builds and runs the app in a docker container for you to interact with.
This will not have access to the host file system and as such you can not save configuration, etc. This is mainly meant for either development or testing out the project.

(Also because earthly is amazing, running the build target afterwards will reuse the cache!)
```bash
earthly +run --release=false
```

## Copy to host
This builds the project and copies it to `./artifacts/arcane`
```bash
earthly +build --release=false
```

## Windows
If you want to run the app on the host machine you need to compile it wihtout earthly. If you dont care about that then `earthly +run --release=false` will also work fine on windows.

It is 100% possible to do a windows build, but `eathly` is not set up for it, install rust locally and run `cargo build`/`cargo run`
