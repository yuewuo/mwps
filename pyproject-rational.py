import sys
import os

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
    (
        "src/python/mwpf/sinter_decoders.py",
        [
            ("import mwpf\n", "import mwpf_rational\n", 1),
            ("from mwpf import", "from mwpf_rational import", 1),
            ("getattr(mwpf, decoder_type)", "getattr(mwpf_rational, decoder_type)", 1),
            ("SinterMWPFDecoder", "SinterMWPFRationalDecoder", 4),
            ("SinterHUFDecoder", "SinterHUFRationalDecoder", 1),
            ("SinterSingleHairDecoder", "SinterSingleHairRationalDecoder", 1),
            ("MwpfCompiledDecoder", "MwpfRationalCompiledDecoder", 3),
        ],
    ),
    (
        "src/python/mwpf/ref_circuit.py",
        [
            ("import mwpf\n", "import mwpf_rational\n", 1),
            ("mwpf.", "mwpf_rational.", None),
        ],
    ),
    (
        "src/python/mwpf/heralded_dem.py",
        [
            ("import mwpf\n", "import mwpf_rational\n", 1),
            ("mwpf.", "mwpf_rational.", None),
        ],
    ),
    (
        "src/python/mwpf/__init__.py",
        [
            ("from .mwpf import *", "from .mwpf_rational import *", 1),
            ("mwpf.", "mwpf_rational.", 2),
            ("mwpf,", "mwpf_rational,", 1),
        ],
    ),
    (
        "tests/python/test_sinter.py",
        [
            ("SinterMWPFDecoder", "SinterMWPFRationalDecoder", None),
            ("SinterHUFDecoder", "SinterHUFRationalDecoder", None),
        ],
    ),
    (
        "README.md",
        [
            ("pip install -U mwpf\n", "pip install -U mwpf_rational\n", 1),
            ('decoders = ["mwpf"],', 'decoders = ["mwpf_rational"],', 1),
            (
                '"mwpf": SinterMWPFDecoder',
                '"mwpf_rational": SinterMWPFRationalDecoder',
                2,
            ),
            ("import SinterMWPFDecoder", "import SinterMWPFRationalDecoder", 1),
            ("from mwpf import ", "from mwpf_rational import ", 2),
        ],
    ),
]

####### module name patches #######
pyclass_patch_files = [
    "src/dual_module.rs",
    "src/example_codes.rs",
    "src/html_export.rs",
    "src/mwpf_solver.rs",
    "src/util_py.rs",
    "src/util.rs",
    "src/visualize.rs",
    "src/matrix/interface.rs",
    "src/matrix/row.rs",
]
for filename in pyclass_patch_files:
    patches.append(
        (
            filename,
            [('pyclass(module = "mwpf"', 'pyclass(module = "mwpf_rational"', None)],
        ),
    )


# patch is strict
def patch(dry: bool):
    for filename, replacements in patches:
        with open(filename, "r") as f:
            content = f.read()
        # check occurrences first
        for old, new, occurrence in replacements:
            assert (
                occurrence is None or content.count(old) == occurrence
            ), f"count {filename} for '{old}': {content.count(old)} != {occurrence}"
            assert (
                content.count(new) == 0
            ), f"count {filename} for '{new}': {content.count(new)} != 0"
        # during application of the replacements, also check occurrence
        for old, new, occurrence in replacements:
            assert occurrence is None or content.count(old) == occurrence
            assert content.count(new) == 0
            old_content = content
            content = content.replace(old, new)
            assert (
                content != old_content
            ), f"Patch failed for {filename}: {old} -> {new}"
        # check occurrences last
        for old, new, occurrence in replacements:
            assert occurrence is None or content.count(new) == occurrence
            assert content.count(old) == 0
        if not dry:
            with open(filename, "w") as f:
                f.write(content)
    if not dry:
        # up to here, all files has been checked and updated, rename the src/python/mwpf folder
        os.rename("src/python/mwpf", "src/python/mwpf_rational")


# revert is best-practice
def revert():
    # first change the folder back
    os.rename("src/python/mwpf_rational", "src/python/mwpf")
    for filename, replacements in patches:
        with open(filename, "r") as f:
            content = f.read()
        for old, new, occurrence in replacements:
            count = content.count(new)
            if occurrence is not None and count != occurrence:
                print(
                    f"[warning] reverting process counting error '{old}' '{new}' {occurrence} != {count}"
                )
            content = content.replace(new, old)
        with open(filename, "w") as f:
            f.write(content)


if __name__ == "__main__":
    assert len(sys.argv) == 2, "Usage: python pyproject-rational.py [dry|apply|revert]"
    if sys.argv[1] == "dry":
        patch(dry=True)
    elif sys.argv[1] == "apply":
        patch(dry=True)
        patch(dry=False)
    elif sys.argv[1] == "revert":
        revert()
    else:
        raise ValueError("Invalid argument, should be dry|apply|revert")
