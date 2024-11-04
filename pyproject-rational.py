import sys

patches = [
    (
        "Cargo.toml",
        [
            ('name = "mwpf"', 'name = "mwpf_rational"'),
            ('default-run = "mwpf"', 'default-run = "mwpf_rational"'),
        ],
    ),
    (
        "src/lib.rs",
        [
            ("fn mwpf(", "fn mwpf_rational("),
        ],
    ),
    (
        "pyproject.toml",
        [
            ('name = "mwpf"', 'name = "mwpf_rational"'),
            ("f64_weight", "rational_weight"),
        ],
    ),
]


def patch(dry):
    for filename, replacements in patches:
        with open(filename, "r") as f:
            content = f.read()
        for old, new in replacements:
            old_content = content
            content = content.replace(old, new)
            assert (
                content != old_content
            ), f"Patch failed for {filename}: {old} -> {new}"
        if not dry:
            with open(filename, "w") as f:
                f.write(content)


if __name__ == "__main__":
    assert len(sys.argv) == 2, "Usage: python pyproject-rational.py [dry|apply]"
    if sys.argv[1] == "dry":
        patch(dry=True)
    elif sys.argv[1] == "apply":
        patch(dry=True)
        patch(dry=False)
    else:
        raise ValueError("Invalid argument, should be dry or apply")
