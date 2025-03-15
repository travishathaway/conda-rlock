# Notes

The following notes are organized as journal entries.

## 2025-02-10

Example of a rattler lock file with pypi dependencies:

- https://github.com/RemiKalbe/IIT-CS579-Project-1/blob/7632dae2f97584b6f6a7710977504623e83b9aed/pixi.lock#L4


### What would be better to have than the current `PackageDb`?

The current `PackageDb` struct does more than I theoretically need it to. It has
to figure out all the different types of packages it's dealing with because later
on in the process it has to figure out how to unpack them. For example, there's a
difference between dealing with source vs. binary distribution.

For my purposes, all I really need is just something that can query the package
index, cache it locally and then allow me to read from it. At the end, I would
just be dealing with `ArtifactInfo` structs which I could then use to
create the lock files I need. The information from the index would then accompany
the metadata I already have about the package on disk in the `*.dist-info` directory.

Right now, I'm not sure whether it's worth it to just write my own Rust that does this
or whether I should still rely on what's in `rip`. I'll run this by Tim in the coming
week to see what he thinks. Also, writing something like this on my own would be informative
as an educational exercise 🤓.

## 2025-02-11

I made some pretty good progress on this today! I decided to just use the `rattler_installs_packages`
crate as-is to see what I could do with it. In doing so, I'm learning a lot about how its
organized. I was almost able to generate lock files with pypi dependencies today.

The only thing left to figure out is how to collect the `requires_dist`. These are the packages
that the locked package depends on. Unfortunately, this isn't available on any on the data
structures that I have been using so far (`ArtifactInfo` and `Distribution`). It is available
on the `WheelCoreMetadata` struct. So, my goal for the next time I sit down with this code is to
figure out how to create these. It will require using code that is able to parse the metadata
contained in the `METADATA` folder.

I think the easiest way to do it from my code is just open the `METADATA` file myself and
just passing the bytes to the `try_from` method.

## 2025-02-12

Finally able to generate a lock file with PyPI dependencies 🥲. The code is a still pretty
messy and needs lots more testing, but thankfully I have reached a very important first milestone.

### TODO

One thing I forgot to do was address the issue with the "platform" specifier. For the time being,
I just put `Platform::Noarch`, but I will eventually need to figure out how to map the value
we get from the package metadata and map it to an appropriate value.

## 2025-02-13

Random notes:

- Learned that it is safe to map subdir to Platform for rattler lock files
- This is how pixi currently maps the python package platform values to conda values:
    - https://github.com/prefix-dev/pixi/blob/86701ad3ba9f3cb3f5779913969c2e1ad6d64bc6/crates/pypi_modifiers/src/pypi_tags.rs
- All the exceptions I raise are PyOSError ones; not sure this is the correct thing to do

### TODO

- I need to start thinking about how I'm going to test this
    - How do I create a conda environment to lock?
    - How do I inspect the lock file I created to verify its validity?
    - How do I test the various error states in the program?
- I still don't have a way to map the PyPI package platform over to the conda platform
    - Might make sense to start working on the `rattler_pypi_interop` crate to see what changes
      I need to have there in order to make this work.
    - One thing to investigate would be to add the platform mapping code from `pixi` to this
      new crate.


## 2025-03-06

### TODO

- Write tests for the `conda.rs` module


## 2025-03-11

We need some more integration tests.
