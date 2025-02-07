import math
import pathlib
from typing import Callable, List, Tuple, Any, Optional, TYPE_CHECKING, Union
import mwpf
from mwpf import (
    SyndromePattern,
    SolverSerialJointSingleHair,
    HyperEdge,
    SolverInitializer,
    Solver,
)
from dataclasses import dataclass

if TYPE_CHECKING:
    import stim
    import numpy as np

available_decoders = [
    "Solver",  # the solver with the highest accuracy, but may change across different versions
    "SolverSerialJointSingleHair",
    "SolverSerialSingleHair",
    "SolverSerialUnionFind",
]

default_cluster_node_limit: int = 50


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
    panic_case: Optional[Tuple[SolverInitializer, SyndromePattern]] = None

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

    def compile_decoder_for_dem(
        self,
        *,
        dem: "stim.DetectorErrorModel",
    ) -> "MwpfCompiledDecoder":
        solver, fault_masks = detector_error_model_to_mwpf_solver_and_fault_masks(
            dem,
            decoder_type=self.decoder_type,
            cluster_node_limit=self._cluster_node_limit,
        )
        return MwpfCompiledDecoder(
            solver,
            fault_masks,
            dem.num_detectors,
            dem.num_observables,
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
        import stim
        import numpy as np

        error_model = stim.DetectorErrorModel.from_file(dem_path)
        solver, fault_masks = detector_error_model_to_mwpf_solver_and_fault_masks(
            error_model,
            decoder_type=self.decoder_type,
            cluster_node_limit=self._cluster_node_limit,
        )
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
                    dets_sparse = np.flatnonzero(
                        np.unpackbits(
                            dets_bit_packed, count=num_dets, bitorder="little"
                        )
                    )
                    syndrome = SyndromePattern(defect_vertices=dets_sparse)
                    if solver is None:
                        prediction = 0
                    else:
                        try:
                            solver.solve(syndrome)
                            subgraph = solver.subgraph()
                            prediction = int(
                                np.bitwise_xor.reduce(fault_masks[subgraph])
                            )
                        except BaseException as e:
                            # record the panic information for debugging use: the panic cases are usually very rare
                            self.panic_case = (solver.get_initializer(), syndrome)
                            raise e  # throw the exception again
                        solver.clear()
                    obs_out_f.write(
                        prediction.to_bytes((num_obs + 7) // 8, byteorder="little")
                    )


@dataclass
class SinterHUFDecoder(SinterMWPFDecoder):
    decoder_type: str = "SolverSerialUnionFind"
    cluster_node_limit: int = 0


@dataclass
class SinterSingleHairDecoder(SinterMWPFDecoder):
    decoder_type: str = "SolverSerialSingleHair"
    cluster_node_limit: int = 0


class MwpfCompiledDecoder:
    def __init__(
        self,
        solver: Union["SolverSerialJointSingleHair", Any],
        fault_masks: "np.ndarray",
        num_dets: int,
        num_obs: int,
    ):
        self.solver = solver
        self.fault_masks = fault_masks
        self.num_dets = num_dets
        self.num_obs = num_obs

    def decode_shots_bit_packed(
        self,
        *,
        bit_packed_detection_event_data: "np.ndarray",
    ) -> "np.ndarray":
        import numpy as np

        num_shots = bit_packed_detection_event_data.shape[0]
        predictions = np.zeros(
            shape=(num_shots, (self.num_obs + 7) // 8), dtype=np.uint8
        )
        for shot in range(num_shots):
            dets_sparse = np.flatnonzero(
                np.unpackbits(
                    bit_packed_detection_event_data[shot],
                    count=self.num_dets,
                    bitorder="little",
                )
            )
            syndrome = SyndromePattern(defect_vertices=dets_sparse)
            if self.solver is None:
                prediction = 0
            else:
                self.solver.solve(syndrome)
                prediction = int(
                    np.bitwise_xor.reduce(self.fault_masks[self.solver.subgraph()])
                )
                self.solver.clear()
            predictions[shot] = np.packbits(
                np.array(
                    list(np.binary_repr(prediction, width=self.num_obs))[::-1],
                    dtype=np.uint8,
                ),
                bitorder="little",
            )
        return predictions


def iter_flatten_model(
    model: "stim.DetectorErrorModel",
    handle_error: Callable[[float, List[int], List[int]], None],
    handle_detector_coords: Callable[[int, "np.ndarray"], None],
):
    import numpy as np
    import stim

    det_offset = 0
    coords_offset = np.zeros(100, dtype=np.float64)

    def _helper(m: "stim.DetectorErrorModel", reps: int):

        nonlocal det_offset
        nonlocal coords_offset
        for _ in range(reps):
            for instruction in m:
                if isinstance(instruction, stim.DemRepeatBlock):
                    _helper(instruction.body_copy(), instruction.repeat_count)
                elif isinstance(instruction, stim.DemInstruction):
                    if instruction.type == "error":
                        dets: set[int] = set()
                        frames: set[int] = set()
                        t: stim.DemTarget
                        p = instruction.args_copy()[0]
                        for t in instruction.targets_copy():
                            if t.is_relative_detector_id():
                                dets ^= {t.val + det_offset}
                            elif t.is_logical_observable_id():
                                frames ^= {t.val}
                        handle_error(p, list(dets), list(frames))
                    elif instruction.type == "shift_detectors":
                        det_offset += instruction.targets_copy()[0]
                        a = np.array(instruction.args_copy())
                        coords_offset[: len(a)] += a
                    elif instruction.type == "detector":
                        a = np.array(instruction.args_copy())
                        for t in instruction.targets_copy():
                            handle_detector_coords(
                                t.val + det_offset, a + coords_offset[: len(a)]
                            )
                    elif instruction.type == "logical_observable":
                        pass
                    else:
                        raise NotImplementedError()
                else:
                    raise NotImplementedError()

    _helper(model, 1)


def deduplicate_hyperedges(
    hyperedges: List[Tuple[List[int], float, int]]
) -> List[Tuple[List[int], float, int]]:
    indices: dict[frozenset[int], Tuple[int, float]] = dict()
    result: List[Tuple[List[int], float, int]] = []
    for dets, weight, mask in hyperedges:
        dets_set = frozenset(dets)
        if dets_set in indices:
            idx, min_weight = indices[dets_set]
            p1 = 1 / (1 + math.exp(weight))
            p2 = 1 / (1 + math.exp(result[idx][1]))
            p = p1 * (1 - p2) + p2 * (1 - p1)
            # choosing the mask from the most likely error
            new_mask = result[idx][2]
            if weight < min_weight:
                indices[dets_set] = (idx, weight)
                new_mask = mask
            result[idx] = (dets, math.log((1 - p) / p), new_mask)
        else:
            indices[dets_set] = (len(result), weight)
            result.append((dets, weight, mask))
    return result


def detector_error_model_to_mwpf_solver_and_fault_masks(
    model: "stim.DetectorErrorModel",
    decoder_type: Any = None,
    cluster_node_limit: int = 50,
) -> Tuple[Optional["SolverSerialJointSingleHair"], "np.ndarray"]:
    """Convert a stim error model into a NetworkX graph."""
    import numpy as np

    num_detectors = model.num_detectors
    is_detector_connected = np.full(num_detectors, False, dtype=bool)
    hyperedges: List[Tuple[List[int], float, int]] = []

    def handle_error(p: float, dets: List[int], frame_changes: List[int]):
        if p == 0:
            return
        if len(dets) == 0:
            # No symptoms for this error.
            # Code probably has distance 1.
            # Accept it and keep going, though of course decoding will probably perform terribly.
            return
        if p > 0.5:
            # mwpf doesn't support negative edge weights (yet, will be supported in the next version).
            # approximate them as weight 0.
            p = 0.5
        weight = math.log((1 - p) / p)
        mask = sum(1 << k for k in frame_changes)
        is_detector_connected[dets] = True
        hyperedges.append((dets, weight, mask))

    def handle_detector_coords(detector: int, coords: "np.ndarray"):
        pass

    iter_flatten_model(
        model,
        handle_error=handle_error,
        handle_detector_coords=handle_detector_coords,
    )
    # mwpf package panic on duplicate edges, thus we need to handle them here
    hyperedges = deduplicate_hyperedges(hyperedges)

    # fix the input by connecting an edge to all isolated vertices; will be supported in the next version
    for idx in range(num_detectors):
        if not is_detector_connected[idx]:
            hyperedges.append(([idx], 0, 0))

    max_weight = max(1e-4, max((w for _, w, _ in hyperedges), default=1))
    rescaled_edges = [
        HyperEdge(v, round(w * 2**10 / max_weight) * 2) for v, w, _ in hyperedges
    ]
    fault_masks = np.array([e[2] for e in hyperedges], dtype=np.uint64)

    initializer = SolverInitializer(
        num_detectors,  # Total number of nodes.
        rescaled_edges,  # Weighted edges.
    )

    if decoder_type is None:
        # default to the solver with highest accuracy
        decoder_cls = Solver
    elif isinstance(decoder_type, str):
        decoder_cls = getattr(mwpf, decoder_type)
    else:
        decoder_cls = decoder_cls
    return (
        (
            decoder_cls(initializer, config={"cluster_node_limit": cluster_node_limit})
            if num_detectors > 0 and len(rescaled_edges) > 0
            else None
        ),
        fault_masks,
    )
