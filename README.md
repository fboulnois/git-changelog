# `git-changelog`

A rust program to generate a `CHANGELOG.md` from semantic git commits.

## How to build:

Type `cargo build --release`.

## How to use:

1. Copy `git-changelog` to `/usr/local/bin` and ensure it is available on the `$PATH`
2. Create an alias to `git-changelog` :

```sh
git config --global alias.changelog /usr/local/bin/git-changelog
```

3. In any git repository, create a `CHANGELOG.md` using `git changelog`
