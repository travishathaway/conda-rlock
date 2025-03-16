# conda-rlock

Welcome to `conda-rlock`! This is an experimental project that aims at bringing
the power of [rattler-lock](https://github.com/conda/rattler) to conda and experimenting
with better overall user experience. If you want a reliable way of using lock files in
conda, please continue using [conda-lock][conda-lock] for this purpose.

## Project goals

- Experiment with UX improvements compared to what conda-lock offers
- Explore how easy it is to build conda plugins with the rattler library
- Support most of what conda-lock supports (e.g. locking pip dependencies,
  pre-solving for other platforms, etc.)
- Start a conversation around unifying conda tools to a [single lock file format][cep-issue-107].

## User experience improvements

As stated above, one of the intended goals of this project is to experiment with different
patterns of using lock files in conda to see if a better user experience can be achieved.
One of the problems with conda-lock currently is that lock file creation is an explicit
action that has to be performed by the user (e.g. by invoking the `conda-lock` command).
With other package managers, such as `npm` and `cargo`, the act of creating lock files
is implicit and users more-or-less never have to think about creating and managing these
files. This is preferred as it relieves users of the additional cognitive load of creating
these files and things _just work_.

When conda-lock was originally created, options for integrating additional features into
conda itself were either limited or non-existent. Since then, conda has added a powerful
plugin framework that gives plugin authors much more power and ability to modify and customize
the behavior of conda. Now, creating a user experience around lock files that mirrors `npm`
and `cargo` is possible.

Below are the improvements this project would like to make over the current experience with
conda-lock:

### Improvement one: transparent lock file creation

> As a user, when I run `conda create|install|update|remove`, I want my lock file to be automatically
> created/updated.

To help us achieve the user story above, we can use the [post command][post-command] plugin hook to
optionally run a lock operation once any of the `create|install|update|remove` commands finish successfully.

We will also want our users to be able to configure this behavior so we can use an additional setting
for this via the [settings][settings] plugin hook:

```yaml
plugins:
  lock_environments: true  # defaults to "false"
```

A file called `conda.lock` would then be created/updated every time any of these commands are run. The
name of the file could also be overridden

#### Open questions

- Where exactly should the `conda.lock` file be written to?
  - Current working directory?
  - Environment root?
- What if I want to handle locking for multiple platforms?
  - Conda's current UX might make this a little awkward as a "post-command" action.

### Improvement two: improved environment creation from a lock file

> As a user, I want to create an environment directly from a lock file by running
> `conda create -n my-env -f conda.lock`

To implement this user story, we will need to use conda's [pre command][pre-command] plugin hook. When this
plugin is installed, it would first check to see if a `--file` option has been passed and if so, determine
whether it is a valid lock file. The plugin would handle creating this environment and then exit the program
early.

The only problem with this approach is that there currently exists no, "clean way" to exit a conda command
earlier than expected other than raising an exception. It would be nice if the plugin could signal the caller
to say something along the lines of, "hey, everything's cool, I just need you to exit a little earlier than
expected because I already took care of everything the user wanted!".

#### Open questions

- Are there any other problems with this approach?
- What's the expected behavior for `conda install|update -f conda.lock` on existing environments?
  - Would the existing environment be removed and replaced with what's in the lock file?

[conda-lock]: https://github.com/conda/conda-lock
[cep-issue-107]: https://github.com/conda/ceps/issues/107
[post-command]: https://docs.conda.io/projects/conda/en/stable/dev-guide/plugins/post_commands.html
[pre-command]: https://docs.conda.io/projects/conda/en/stable/dev-guide/plugins/pre_commands.html
[settings]: https://docs.conda.io/projects/conda/en/stable/dev-guide/plugins/settings.html
