# Notes

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
