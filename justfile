flake-ci:
    git branch -D update_flake_lock_action
    git fetch origin
    git checkout update_flake_lock_action
    git commit --amend --no-edit
    git push origin update_flake_lock_action --force

flake-ci-init:
    git fetch origin
    git checkout update_flake_lock_action
    git commit --amend --no-edit
    git push origin update_flake_lock_action --force

