from common import *
import stim
import sinter
import numpy as np


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
    [0.001],
)
@pytest.mark.parametrize(
    "d",
    [3],
)
@pytest.mark.parametrize(
    "code_type",
    [
        "color_code:memory_xyz",
        "repetition_code:memory",
        "surface_code:rotated_memory_x",
    ],
)
def test_sinter_decode(decoder: sinter.Decoder, p: float, d: int, code_type: str):
    if "mwpf" not in sys.modules:
        print("[skip] because sinter would fail to import without the mwpf package")
        print(
            "once https://github.com/quantumlib/Stim/pull/873 is merged, this test should be enabled"
        )
        return

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


def test_sinter_heralded_error():
    circuit = stim.Circuit(
        """\
# the problem of the default handling heralded error is that the heralded detector
# would force the edge to be selected even if there is a more probably error set
R 0 1
X_ERROR(0.01) 0  # with lower probability, an error happens at 0
X_ERROR(0.1) 1  # normally we would always guess that the actual error happens at 1
HERALDED_PAULI_CHANNEL_1(0, 0.02, 0, 0) 0  # D0
DETECTOR rec[-1]
MPP Z0*Z1  # D1
DETECTOR rec[-1]
M 0
OBSERVABLE_INCLUDE(0) rec[-1]  # read qubit 0
"""
    )
    print(circuit)
    dem = circuit.detector_error_model()
    print("######### dem #########")
    print(dem)
    # without special handling, the dem includes the heralded error as regular pauli error
    # when decoded natively, we always think that
    assert dem == stim.DetectorErrorModel(
        """
error(0.02) D0 D1 L0  # herald detector D0 uniquely determines whether the hyperedge { D1 } occurs
error(0.1) D1
error(0.01) D1 L0
"""
    )
    decoder = mwpf.SinterDevMWPFDecoder()
    compiled_decoder = decoder.compile_decoder_for_dem(dem=dem)
    prediction = compiled_decoder.decode_shots_bit_packed(
        bit_packed_detection_event_data=np.packbits(
            np.array([[1, 1]]), axis=-1, bitorder="little"
        )
    )
    observables = np.unpackbits(
        prediction[0], count=dem.num_observables, bitorder="little"
    )
    # the decoder wrongly select the heralded error, despite there is a more probable solution error(0.1) D1
    assert observables == [1]

    # now let's tell the decoder about the circuit itself so that it can extract the heralded error information
    # decoder.circuit =
