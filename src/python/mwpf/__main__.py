import mwpf, sys

# example: python -m mwpf --help

if __name__ == "__main__":
    # sys.argv will not contain things like "python -m"
    mwpf.run_cli(sys.argv)
