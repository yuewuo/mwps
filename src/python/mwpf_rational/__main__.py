import mwpf_rational, sys

# example: python -m mwpf_rational --help

if __name__ == "__main__":
    # sys.argv will not contain things like "python -m"
    mwpf_rational.run_cli(sys.argv)
