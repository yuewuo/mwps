import math
import pathlib
from typing import Tuple, Any, Optional
import mwpf
from mwpf import (  # type: ignore
    SyndromePattern,
    HyperEdge,
    SolverInitializer,
    Solver,
    BP,
)
from dataclasses import dataclass, field
import pickle
import json
import traceback
from enum import Enum
import random
import numpy as np
import stim

from .ref_circuit import *
from .heralded_dem import *

available_decoders = [
    "Solver",  # the solver with the highest accuracy, but may change across different versions
    "SolverSerialJointSingleHair",
    "SolverSerialSingleHair",
    "SolverSerialUnionFind",
]

default_cluster_node_limit: int = 50


@dataclass
class DecoderPanic:
    initializer: SolverInitializer
    config: dict
    syndrome: SyndromePattern
    panic_message: str


class PanicAction(Enum):
    RAISE = 1  # raise the panic with proper message to help debugging
    CATCH = 2  # proceed with normal decoding and return all-0 result


@dataclass
class SinterMWPFDecoder:
    """
    Use MWPF to predict observables from detection events.

    Args:
        decoder_type: decoder class used to construct the MWPF decoder.  in the Rust implementation, all of them inherits from the class of `SolverSerialPlugins` but just provide different plugins for optimizing the primal and/or dual solutions. For example, `SolverSerialUnionFind` is the most basic solver without any plugin: it only grows the clusters until the first valid solution appears; some more optimized solvers uses one or more plugins to further optimize the solution, which requires longer decoding time.

        cluster_node_limit (alias: c): The maximum number of nodes in a cluster, used to tune the performance of the decoder. The default value is 50.
    """

    decoder_type: str = "SolverSerialJointSingleHair"
    cluster_node_limit: Optional[int] = None
    c: Optional[int] = None  # alias of `cluster_node_limit`, will override it
    timeout: Optional[float] = None
    with_progress: bool = False
    circuit: Optional[stim.Circuit] = None  # RefCircuit is not picklable
    # this parameter itself doesn't do anything to load the circuit but only check whether the circuit is indeed loaded
    pass_circuit: bool = False

    # record panic data and controls whether the raise the panic or simply record them
    panic_action: PanicAction = PanicAction.CATCH
    panic_cases: list[DecoderPanic] = field(default_factory=list)

    @property
    def _cluster_node_limit(self) -> int:
        if self.cluster_node_limit is not None:
            assert self.c is None, "Cannot set both `cluster_node_limit` and `c`."
            return self.cluster_node_limit
        elif self.c is not None:
            assert (
                self.cluster_node_limit is None
            ), "Cannot set both `cluster_node_limit` and `c`."
            return self.c
        return default_cluster_node_limit

    @property
    def config(self) -> dict[str, Any]:
        return dict(cluster_node_limit=self._cluster_node_limit)

    def with_circuit(self, circuit: stim.Circuit | None) -> "SinterMWPFDecoder":
        if circuit is None:
            self.circuit = None
            return self
        assert isinstance(circuit, stim.Circuit)
        self.circuit = circuit.copy()
        return self

    def compile_decoder_for_dem(
        self,
        *,
        dem: "stim.DetectorErrorModel",
    ) -> "MwpfCompiledDecoder":
        if self.pass_circuit:
            assert (
                self.circuit is not None
            ), "The circuit is not loaded but the flag `pass_circuit` is True"

        solver, predictor = construct_decoder_and_predictor(
            dem,
            decoder_type=self.decoder_type,
            config=self.config,
            ref_circuit=(
                RefCircuit.of(self.circuit) if self.circuit is not None else None
            ),
        )
        assert (
            dem.num_detectors == predictor.num_detectors()
        ), "Mismatched number of detectors, are you using the corresponding circuit of dem?"
        assert (
            dem.num_observables == predictor.num_observables()
        ), "Mismatched number of observables, are you using the corresponding circuit of dem?"
        return MwpfCompiledDecoder(
            solver,
            predictor,
            dem.num_detectors,
            dem.num_observables,
            panic_action=self.panic_action,
            panic_cases=self.panic_cases,  # record all the panic information to the same place
        )

    def decode_via_files(
        self,
        *,
        num_shots: int,
        num_dets: int,
        num_obs: int,
        dem_path: pathlib.Path,
        dets_b8_in_path: pathlib.Path,
        obs_predictions_b8_out_path: pathlib.Path,
        tmp_dir: pathlib.Path,
    ) -> None:
        if self.pass_circuit:
            assert (
                self.circuit is not None
            ), "The circuit is not loaded but the flag `pass_circuit` is True"

        dem = stim.DetectorErrorModel.from_file(dem_path)
        solver, predictor = construct_decoder_and_predictor(
            dem,
            decoder_type=self.decoder_type,
            config=self.config,
            ref_circuit=(
                RefCircuit.of(self.circuit) if self.circuit is not None else None
            ),
        )
        assert num_dets == predictor.num_detectors()
        assert num_obs == predictor.num_observables()

        num_det_bytes = math.ceil(num_dets / 8)
        with open(dets_b8_in_path, "rb") as dets_in_f:
            with open(obs_predictions_b8_out_path, "wb") as obs_out_f:
                if self.with_progress:
                    from tqdm import tqdm

                    pbar = tqdm(total=num_shots, desc="shots")
                for _ in range(num_shots):
                    if self.with_progress:
                        pbar.update(1)
                    dets_bit_packed = np.fromfile(
                        dets_in_f, dtype=np.uint8, count=num_det_bytes
                    )
                    if dets_bit_packed.shape != (num_det_bytes,):
                        raise IOError("Missing dets data.")
                    syndrome = predictor.syndrome_of(dets_bit_packed)
                    if solver is None:
                        prediction = 0
                    else:
                        try:
                            solver.solve(syndrome)
                            subgraph = solver.subgraph()
                            prediction = predictor.prediction_of(syndrome, subgraph)
                        except BaseException as e:
                            self.panic_cases.append(
                                DecoderPanic(
                                    initializer=solver.get_initializer(),
                                    config=solver.config,
                                    syndrome=syndrome,
                                    panic_message=traceback.format_exc(),
                                )
                            )
                            if "<class 'KeyboardInterrupt'>" in str(e):
                                raise e
                            elif self.panic_action == PanicAction.RAISE:
                                raise ValueError(panic_text_of(solver, syndrome)) from e
                            elif self.panic_action == PanicAction.CATCH:
                                prediction = random.getrandbits(num_obs)
                    obs_out_f.write(
                        int(prediction).to_bytes((num_obs + 7) // 8, byteorder="little")
                    )


def construct_decoder_and_predictor(
    model: "stim.DetectorErrorModel",
    decoder_type: Any,
    config: dict[str, Any],
    ref_circuit: Optional[RefCircuit] = None,
) -> Tuple[Any, Predictor]:

    if ref_circuit is not None:
        heralded_dem = HeraldedDetectorErrorModel(ref_circuit=ref_circuit)
        initializer = heralded_dem.initializer
        predictor: Predictor = heralded_dem.predictor
    else:
        ref_dem = RefDetectorErrorModel.of(dem=model)
        initializer = ref_dem.initializer
        predictor = ref_dem.predictor

    if decoder_type is None:
        # default to the solver with highest accuracy
        decoder_cls = Solver
    elif isinstance(decoder_type, str):
        decoder_cls = getattr(mwpf, decoder_type)
    else:
        decoder_cls = decoder_cls
    return (
        decoder_cls(initializer, config=config),
        predictor,
    )


def panic_text_of(solver, syndrome) -> str:
    initializer = solver.get_initializer()
    config = solver.config
    syndrome
    panic_text = f"""
######## MWPF Sinter Decoder Panic ######## 
solver_initializer: dict = json.loads('{initializer.to_json()}')
config: dict = json.loads('{json.dumps(config)}')
syndrome: dict = json.loads('{syndrome.to_json()}')
######## PICKLE DATA ######## 
solver_initializer: SolverInitializer = pickle.loads({pickle.dumps(initializer)!r})
config: dict = pickle.loads({pickle.dumps(config)!r})
syndrome: SyndromePattern = pickle.loads({pickle.dumps(syndrome)!r})
######## End Panic Information ######## 
"""
    return panic_text


@dataclass
class SinterHUFDecoder(SinterMWPFDecoder):
    decoder_type: str = "SolverSerialUnionFind"
    cluster_node_limit: int = 0


@dataclass
class SinterSingleHairDecoder(SinterMWPFDecoder):
    decoder_type: str = "SolverSerialSingleHair"
    cluster_node_limit: int = 0


@dataclass
class MwpfCompiledDecoder:
    solver: Any
    predictor: Predictor
    num_dets: int
    num_obs: int
    panic_action: PanicAction = PanicAction.CATCH
    panic_cases: list[DecoderPanic] = field(default_factory=list)

    def decode_shots_bit_packed(
        self,
        *,
        bit_packed_detection_event_data: "np.ndarray",
    ) -> "np.ndarray":
        num_shots = bit_packed_detection_event_data.shape[0]
        predictions = np.zeros(
            shape=(num_shots, (self.num_obs + 7) // 8), dtype=np.uint8
        )
        for shot in range(num_shots):
            syndrome = self.predictor.syndrome_of(bit_packed_detection_event_data[shot])
            if self.solver is None:
                prediction = 0
            else:
                try:
                    self.solver.solve(syndrome)
                    subgraph = self.solver.subgraph()
                    prediction = self.predictor.prediction_of(syndrome, subgraph)
                except BaseException as e:
                    self.panic_cases.append(
                        DecoderPanic(
                            initializer=self.solver.get_initializer(),
                            config=self.solver.config,
                            syndrome=syndrome,
                            panic_message=traceback.format_exc(),
                        )
                    )
                    if "<class 'KeyboardInterrupt'>" in str(e):
                        raise e
                    elif self.panic_action == PanicAction.RAISE:
                        raise ValueError(panic_text_of(self.solver, syndrome)) from e
                    elif self.panic_action == PanicAction.CATCH:
                        prediction = random.getrandbits(self.num_obs)
            predictions[shot] = np.packbits(
                np.array(
                    list(np.binary_repr(prediction, width=self.num_obs))[::-1],
                    dtype=np.uint8,
                ),
                bitorder="little",
            )
        return predictions


@dataclass
class SinterBPMWPFDecoder(SinterMWPFDecoder):
    max_iter: int = 10
    bp_application_ratio: float = 0.625

    def decode_via_files(
        self,
        *,
        num_shots: int,
        num_dets: int,
        num_obs: int,
        dem_path: pathlib.Path,
        dets_b8_in_path: pathlib.Path,
        obs_predictions_b8_out_path: pathlib.Path,
        tmp_dir: pathlib.Path,
    ) -> None:
        # TODO: need better code structure to avoid writing duplicate code
        raise NotImplemented("Not implemented for SinterBPMWPFDecoder")

    def compile_decoder_for_dem(
        self,
        *,
        dem: "stim.DetectorErrorModel",
    ) -> "BPMWPFCompiledDecoder":
        compiled_decoder = super().compile_decoder_for_dem(dem=dem)
        return BPMWPFCompiledDecoder(
            mwpf_decoder=compiled_decoder,
            max_iter=self.max_iter,
            bp_application_ratio=self.bp_application,
        )


@dataclass
class BPMWPFCompiledDecoder:
    mwpf_decoder: MwpfCompiledDecoder
    max_iter: int = 10
    bp_application_ratio: float = 0.625

    def __post_init__(self):
        self.bp_decoder = MwpfCompiledDecoder(
            solver=BP(
                self.mwpf_decoder.solver.get_solver_base(),
                max_iter=self.max_iter,
                bp_application_ratio=self.bp_application_ratio,
            )
        )

    def decode_shots_bit_packed(
        self,
        *,
        bit_packed_detection_event_data: "np.ndarray",
    ) -> "np.ndarray":
        return self.bp_decoder.decode_shots_bit_packed(
            bit_packed_detection_event_data=bit_packed_detection_event_data
        )
