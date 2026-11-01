"""The workflows: a failing command before a pipe fails its step. GitHub runs a step's script with
`bash -e` unless `shell: bash` is named (then `bash -eo pipefail`); `out=$(script | tail -n 1)`
without pipefail passes on the script's failure."""
import glob
import os
import re

import yaml

import project as P

WORKFLOWS = sorted(glob.glob(f"{P.REPO}/.github/workflows/*.yml"))


def shell_of(*levels):
    for level in levels:
        shell = ((level or {}).get("defaults") or {}).get("run", {}).get("shell")
        if shell:
            return shell
    return None


def piped(script):
    # a pipe, not `||` nor a pipe inside a regular expression's quotes
    unquoted = re.sub(r"'[^']*'|\"[^\"]*\"", "", script)
    return re.search(r"(?<!\|)\|(?!\|)", unquoted) is not None


def test_a_failing_command_before_a_pipe_fails_its_step():
    unsafe = []
    for path in WORKFLOWS:
        flow = yaml.safe_load(open(path))
        for name, job in flow["jobs"].items():
            for step in job.get("steps", []):
                script = step.get("run")
                if not script or not piped(script):
                    continue
                shell = step.get("shell") or shell_of(job, flow)
                if shell == "bash" or shell == "pwsh" or "set -o pipefail" in script:
                    continue
                unsafe.append(f"{os.path.basename(path)} {name}: {step.get('name', script.splitlines()[0])}")
    assert unsafe == []
