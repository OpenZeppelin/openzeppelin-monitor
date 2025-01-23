# Contributing

Thank you for your interest in contributing to the OpenZeppelin Monitor project! This document provides guidelines to ensure your contributions are effectively integrated into the project.

There are many ways to contribute, regardless of your experience level. Whether you're new to Rust or a seasoned expert, your help is invaluable. Every contribution matters, no matter how small, and all efforts are greatly appreciated. This document is here to guide you through the process. Don’t feel overwhelmed—it’s meant to support and simplify your contribution journey.

- [Contributing](#contributing)
  - [Communication](#communication)
  - [Development Workflow](#development-workflow)
  - [GitHub workflow](#github-workflow)
    - [1. Fork in the cloud](#1-fork-in-the-cloud)
    - [2. Clone fork to local storage](#2-clone-fork-to-local-storage)
    - [3. Create a Working Branch](#3-create-a-working-branch)
    - [4. Keep your branch in sync](#4-keep-your-branch-in-sync)
    - [5. Commit Your Changes](#5-commit-your-changes)
    - [6. Push to GitHub](#6-push-to-github)
    - [7. Create a Pull Request](#7-create-a-pull-request)
    - [Get a code review](#get-a-code-review)
    - [Squash commits](#squash-commits)
    - [Merging a commit](#merging-a-commit)
    - [Reverting a commit](#reverting-a-commit)
    - [Opening a Pull Request](#opening-a-pull-request)
  - [Code Review](#code-review)
  - [Best practices](#best-practices)
  - [Coding Standards](#coding-standards)
  - [Testing](#testing)
  - [Security](#security)
  - [Documentation](#documentation)
  - [Issues Management or Triage](#issues-management-or-triage)
  - [License](#license)
  - [Code of Conduct](#code-of-conduct)

OpenZeppelin Monitor is open source and welcomes contributions from the community.

As a potential contributor, your changes and ideas are welcome at any hour of the day or night, weekdays, weekends, and holidays.
Please do not ever hesitate to ask a question or send a pull request.

Beginner focused information can be found below in [Open a Pull Request](#opening-a-pull-request) and [Code Review](#code-review).

## Communication

- [CODEOWNERS](./CODEOWNERS)
- [Email](defender-support@openzeppelin.com)
- [Website](https://openzeppelin.com/)
- [Blog](https://blog.openzeppelin.com/)
- [X](https://x.com/OpenZeppelin)

## Development Workflow

1. **Set Up Development Environment**:
   - Install dependencies:

     ```sh
     cargo build
     ```

   - Set up environment variables:

     ```sh
     cp .env.example .env
     ```

2. **Run Tests**:
   - Unit tests:

     ```sh
     cargo test
     ```

   - Integration tests:

     ```sh
     cargo test integration
     ```

3. **Follow Git Hooks**:
   - Make hooks executable:

     ```sh
     chmod +x .githooks/*
     ```

   - Configure hooks:

     ```sh
     git config core.hooksPath .githooks
     ```

## GitHub workflow

### 1. Fork in the cloud

1. Visit <https://github.com/openzeppelin/openzeppelin-monitor>
2. Click `Fork` button (top right) to establish a cloud-based fork.

### 2. Clone fork to local storage

In your shell, define a local working directory as `working_dir`.

```sh
export working_dir="${HOME}/repos" # Change to your preferred location for source code
```

Set `user` to match your github profile name:

```sh
export user=<your github profile name>
```

Create your clone:

```sh
mkdir -p $working_dir
cd $working_dir
git clone https://github.com/$user/openzeppelin-monitor.git
# or: git clone git@github.com:$user/openzeppelin-monitor.git

cd $working_dir/openzeppelin-monitor
git remote add upstream https://github.com/openzeppelin/openzeppelin-monitor.git
# or: git remote add upstream git@github.com:openzeppelin/openzeppelin-monitor.git

# Never push to upstream main
git remote set-url --push upstream no_push

# Confirm that your remotes make sense:
git remote -v
```

### 3. Create a Working Branch

Get your local master up to date. Note that depending on which repository you are working from,
the default branch may be called "main" instead of "master".

```sh
cd $working_dir/openzeppelin-monitor
git fetch upstream
git checkout main
git rebase upstream/main
```

Create your new branch.

```sh
git checkout -b myfeature
# or git switch -c myfeature
```

You may now edit files on the `myfeature` branch.

### 4. Keep your branch in sync

You will need to periodically fetch changes from the `upstream`
repository to keep your working branch in sync. Note that depending on which repository you are working from,
the default branch may be called 'main' instead of 'master'.

Make sure your local repository is on your working branch and run the
following commands to keep it in sync:

```sh
git fetch upstream
git rebase upstream/main
```

Please don't use `git pull` instead of the above `fetch` and
`rebase`. Since `git pull` executes a merge, it creates merge commits. These make the commit history messy
and violate the principle that commits ought to be individually understandable
and useful (see below).

You might also consider changing your `.git/config` file via
`git config branch.autoSetupRebase always` to change the behavior of `git pull`, or another non-merge option such as `git pull --rebase`.

### 5. Commit Your Changes

You will probably want to regularly commit your changes. It is likely that you will go back and edit,
build, and test multiple times. After a few cycles of this, you might
[amend your previous commit](https://www.w3schools.com/git/git_amend.asp).

```sh
git commit
```

**signing commits**
We use signed commits enforcement as a best practice. Make sure to sign your commits. This is a requirement for all commits. You can read more about signing commits [here](https://help.github.com/en/github/authenticating-to-github/signing-commits).

```sh
git commit -s
```

### 6. Push to GitHub

When your changes are ready for review, push your working branch to
your fork on GitHub.

```sh
git push -f <your_remote_name> myfeature
```

### 7. Create a Pull Request

1. Visit your fork at `https://github.com/<user>/openzeppelin-monitor`
2. Click the **Compare & Pull Request** button next to your `myfeature` branch.

_If you have upstream write access_, please refrain from using the GitHub UI for
creating PRs, because GitHub will create the PR branch inside the main
repository rather than inside your fork.

### Get a code review

Once your pull request has been opened it will be assigned to one or more
reviewers.  Those reviewers will do a thorough code review, looking for
correctness, bugs, opportunities for improvement, documentation and comments,
and style.

Commit changes made in response to review comments to the same branch on your
fork.

Very small PRs are easy to review.  Very large PRs are very difficult to review.

### Squash commits

After a review, prepare your PR for merging by squashing your commits.

All commits left on your branch after a review should represent meaningful milestones or units of work. Use commits to add clarity to the development and review process.

Before merging a PR, squash the following kinds of commits:

- Fixes/review feedback
- Typos
- Merges and rebases
- Work in progress

Aim to have every commit in a PR compile and pass tests independently if you can, but it's not a requirement. In particular, `merge` commits must be removed, as they will not pass tests.

To squash your commits, perform an [interactive rebase](https://git-scm.com/book/en/v2/Git-Tools-Rewriting-History):

1. Check your git branch:

  ```sh
  git status
  ```

  The output should be similar to this:

  ```sh
  On branch your-contribution
  Your branch is up to date with 'origin/your-contribution'.
  ```

1. Start an interactive rebase using a specific commit hash, or count backwards from your last commit using `HEAD~<n>`, where `<n>` represents the number of commits to include in the rebase.

  ```sh
  git rebase -i HEAD~3
  ```

  The output should be similar to this:

  ```sh
  pick 2ebe926 Original commit
  pick 31f33e9 Address feedback
  pick b0315fe Second unit of work

  # Rebase 7c34fc9..b0315ff onto 7c34fc9 (3 commands)
  #
  # Commands:
  # p, pick <commit> = use commit
  # r, reword <commit> = use commit, but edit the commit message
  # e, edit <commit> = use commit, but stop for amending
  # s, squash <commit> = use commit, but meld into previous commit
  # f, fixup <commit> = like "squash", but discard this commit's log message

  ...

  ```

1. Use a command line text editor to change the word `pick` to `squash` for the commits you want to squash, then save your changes and continue the rebase:

  ```sh
  pick 2ebe926 Original commit
  squash 31f33e9 Address feedback
  pick b0315fe Second unit of work

  ...

  ```

  The output after saving changes should look similar to this:

  ```sh
  [detached HEAD 61fdded] Second unit of work
   Date: Thu Mar 5 19:01:32 2020 +0100
   2 files changed, 15 insertions(+), 1 deletion(-)

   ...

  Successfully rebased and updated refs/heads/main.
  ```

1. Force push your changes to your remote branch:

  ```sh
  git push --force-with-lease
  ```

For mass automated fixups such as automated doc formatting, use one or more
commits for the changes to tooling and a final commit to apply the fixup en
masse. This makes reviews easier.

An alternative to this manual squashing process is to use the Prow and Tide based automation that is configured in GitHub: adding a comment to your PR with `/label tide/merge-method-squash` will trigger the automation so that GitHub squash your commits onto the target branch once the PR is approved. Using this approach simplifies things for those less familiar with Git, but there are situations in where it's better to squash locally; reviewers will have this in mind and can ask for manual squashing to be done.

By squashing locally, you control the commit message(s) for your work, and can separate a large PR into logically separate changes.
For example: you have a pull request that is code complete and has 24 commits. You rebase this against the same merge base, simplifying the change to two commits. Each of those two commits represents a single logical change and each commit message summarizes what changes. Reviewers see that the set of changes are now understandable, and approve your PR.

### Merging a commit

Once you've received review and approval, your commits are squashed, your PR is ready for merging.

Merging happens automatically after both a Reviewer and Approver have approved the PR. If you haven't squashed your commits, they may ask you to do so before approving a PR.

### Reverting a commit

In case you wish to revert a commit, use the following instructions.

_If you have upstream write access_, please refrain from using the
`Revert` button in the GitHub UI for creating the PR, because GitHub
will create the PR branch inside the main repository rather than inside your fork.

- Create a branch and sync it with upstream. Note that depending on which repository you are working from, the default branch may be called 'main' instead of 'master'.

  ```sh
  # create a branch
  git checkout -b myrevert

  # sync the branch with upstream
  git fetch upstream
  git rebase upstream/master
  ```

- If the commit you wish to revert is a _merge commit_, use this command:

  ```sh
  # SHA is the hash of the merge commit you wish to revert
  git revert -m 1 <SHA>
  ```

  If it is a _single commit_, use this command:

  ```sh
  # SHA is the hash of the single commit you wish to revert
  git revert <SHA>
  ```

- This will create a new commit reverting the changes. Push this new commit to your remote.

  ```sh
  git push <your_remote_name> myrevert
  ```

- Finally, [create a Pull Request](#7-create-a-pull-request) using this branch.

### Opening a Pull Request

Pull requests are often called a "PR".
OpenZeppelin Monitor generally follows the standard [github pull request](https://help.github.com/articles/about-pull-requests/) process, but there is a layer of additional specific differences:

Common new contributor PR issues are:

- Dealing with test cases which fail on your PR, unrelated to the changes you introduce.
- Include mentions (like @person) and [keywords](https://help.github.com/en/articles/closing-issues-using-keywords) which could close the issue (like fixes #xxxx) in commit messages.

## Code Review

As a community we believe in the value of code review for all contributions.
Code review increases both the quality and readability of our codebase, which
in turn produces high quality software.

As a community we expect that all active participants in the
community will also be active reviewers.

There are two aspects of code review: giving and receiving.

To make it easier for your PR to receive reviews, consider the reviewers will need you to:

- Write [good commit messages](https://chris.beams.io/posts/git-commit/)
- Break large changes into a logical series of smaller patches which individually make easily understandable changes, and in aggregate solve a broader issue
- Label PRs: to do this read the messages the bot sends you to guide you through the PR process

Reviewers, the people giving the review, are highly encouraged to revisit the [Code of Conduct](./CODE_OF_CONDUCT.md) and must go above and beyond to promote a collaborative, respectful community.
When reviewing PRs from others [The Gentle Art of Patch Review](http://sage.thesharps.us/2014/09/01/the-gentle-art-of-patch-review/) suggests an iterative series of focuses which is designed to lead new contributors to positive collaboration without inundating them initially with nuances:

- Is the idea behind the contribution sound?
- Is the contribution architected correctly?
- Is the contribution polished?

Note: if your pull request isn't getting enough attention, you can email us at `defender-support@openzeppelin.com` to get help finding reviewers.

## Best practices

- Write clear and meaningful git commit messages.
- If the PR will _completely_ fix a specific issue, include `fixes #123` in the PR body (where 123 is the specific issue number the PR will fix. This will automatically close the issue when the PR is merged.
- Make sure you don't include `@mentions` or `fixes` keywords in your git commit messages. These should be included in the PR body instead.
- When you make a PR for small change (such as fixing a typo, style change, or grammar fix), please squash your commits so that we can maintain a cleaner git history.
- Make sure you include a clear and detailed PR description explaining the reasons for the changes, and ensuring there is sufficient information for the reviewer to understand your PR.
- Additional Readings:
  - [chris.beams.io/posts/git-commit/](https://chris.beams.io/posts/git-commit/)
  - [github.com/blog/1506-closing-issues-via-pull-requests](https://github.com/blog/1506-closing-issues-via-pull-requests)
  - [davidwalsh.name/squash-commits-git](https://davidwalsh.name/squash-commits-git)
  - [https://mtlynch.io/code-review-love/](https://mtlynch.io/code-review-love/)

## Coding Standards

- Use **Rust 2021 edition**.
- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Format code with `rustfmt`:

  ```sh
  rustup component add rustfmt --toolchain nightly
  cargo +nightly fmt
  ```

- Lint code with `clippy`:

  ```sh
  cargo clippy --all-targets --all-features
  ```

## Testing

Testing is the responsibility of all contributors as such all contributions must pass existing tests and include new tests when applicable:

1. Write tests for new features or bug fixes.
2. Run the test suite:

   ```sh
   cargo test
   ```

3. Ensure no warnings or errors.

## Security

- Follow the stated [Security Policy](SECURITY.md).

## Documentation

- TBD

## Issues Management or Triage

- TBD

## License

By contributing to this project, you agree that your contributions will be licensed under the [AGPL-3.0 License](LICENSE).

## Code of Conduct

This project and everyone participating in it is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report any unacceptable behavior to `defender-support@openzeppelin.com`
