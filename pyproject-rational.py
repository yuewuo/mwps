import sys

patches = [
    (
        "Cargo.toml",
        [
            ('name = "mwpf"', 'name = "mwpf_rational"', 3),
            ('default-run = "mwpf"', 'default-run = "mwpf_rational"', 1),
        ],
    ),
    (
        "src/lib.rs",
        [
            ("fn mwpf(", "fn mwpf_rational(", 1),
        ],
    ),
    (
        "src/main.rs",
        [
            ("use mwpf::cli::*;", "use mwpf_rational::cli::*;", 1),
        ],
    ),
    (
        "pyproject.toml",
        [
            ('name = "mwpf"', 'name = "mwpf_rational"', 1),
            ("f64_weight", "rational_weight", 2),
        ],
    ),
]


# patch is strict
def patch(dry):
    for filename, replacements in patches:
        with open(filename, "r") as f:
            content = f.read()
        # check occurrences first
        for old, new, occurrence in replacements:
            assert (
                content.count(old) == occurrence
            ), f"count {filename} for '{old}': {content.count(old)} != {occurrence}"
            assert (
                content.count(new) == 0
            ), f"count {filename} for '{new}': {content.count(new)} != 0"
        # during application of the replacements, also check occurrence
        for old, new, occurrence in replacements:
            assert content.count(old) == occurrence
            assert content.count(new) == 0
            old_content = content
            content = content.replace(old, new)
            assert (
                content != old_content
            ), f"Patch failed for {filename}: {old} -> {new}"
        # check occurrences last
        for old, new, occurrence in replacements:
            assert content.count(new) == occurrence
            assert content.count(old) == 0
        if not dry:
            with open(filename, "w") as f:
                f.write(content)


# revert is best-practice
def revert():
    for filename, replacements in patches:
        with open(filename, "r") as f:
            content = f.read()
        for old, new, occurrence in replacements:
            count = content.count(new)
            if count != occurrence:
                print(
                    f"[warning] reverting process counting error '{old}' '{new}' {occurrence} != {count}"
                )
            content = content.replace(new, old)
        with open(filename, "w") as f:
            f.write(content)


if __name__ == "__main__":
    assert len(sys.argv) == 2, "Usage: python pyproject-rational.py [dry|apply]"
    if sys.argv[1] == "dry":
        patch(dry=True)
    elif sys.argv[1] == "apply":
        patch(dry=True)
        patch(dry=False)
    elif sys.argv[1] == "revert":
        revert()
    else:
        raise ValueError("Invalid argument, should be dry or apply")
