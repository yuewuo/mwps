from common import *
import stim


@pytest.mark.parametrize(
    "decoder",
    [
        mwpf.SinterMWPFDecoder(cluster_node_limit=50),
        mwpf.SinterHUFDecoder(),
        mwpf.SinterMWPFDecoder(cluster_node_limit=0),
    ],
)
@pytest.mark.parametrize(
    "p",
    [0.001, 0.01],
)
@pytest.mark.parametrize(
    "d",
    [3, 5],
)
@pytest.mark.parametrize(
    "code_type",
    [
        "color_code:memory_xyz",
        "repetition_code:memory",
        "surface_code:rotated_memory_x",
    ],
)
def test_sinter_decode(decoder: "sinter.Decoder", p: float, d: int, code_type: str):
    if "mwpf" not in sys.modules:
        print("[skip] because sinter would fail to import")
        return

    import sinter

    # pytest -s tests/python/test_sinter.py::test_sinter_decode
    circuit = stim.Circuit.generated(
        code_type,
        rounds=d,
        distance=d,
        after_clifford_depolarization=p,
        before_round_data_depolarization=p,
        before_measure_flip_probability=p,
        after_reset_flip_probability=p,
    )

    task = sinter.Task(
        circuit=circuit,
        collection_options=sinter.CollectionOptions(max_shots=10, max_errors=10),
    )

    sinter.collect(
        num_workers=2,
        tasks=[task],
        decoders=["mwpf"],
        custom_decoders={"mwpf": decoder},
    )
