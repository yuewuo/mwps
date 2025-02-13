"""
Regular Detector Error Model (DEM) does not contain the information of the heralded error.
This HeraldedDetectorErrorModel class provides additional information on the heralded errors.
It is capable of reading bits from the detector which corresponds to the heralded error indicator.

Note that in order to let the tool read a heralded error in the circuit, it is required that 
the heralded error is detected using `DETECTOR rec[...]` where `rec[...]` corresponds to the heralded event.
To help user, we provide a function that automatically adds such detections.

[warning] not all circuits can be used in this class. Notably, there are several requirements:
1. the measurement of a heralded error must be either not detected or uniquely detected by a detector
``
"""

import stim
from .ref_circuit import (
    RefCircuit,
    RefInstruction,
    RefRec,
    RefDetector,
    RefDetectorErrorModel,
)
import functools
from dataclasses import dataclass
from frozendict import frozendict


DEM_MIN_PROBABILITY = 1e-15  # below this value, DEM starts to ignore the error rate


# avoid non-zero small probability to be ignored by the DEM
def dem_probability(probability: float) -> float:
    if probability == 0:
        return 0.0
    return max(DEM_MIN_PROBABILITY, probability)


@dataclass(frozen=True)
class HeraldedDetectorErrorModel:
    ref_circuit: RefCircuit
    # We assume that the detector error model has a non-zero false positive rate to make the decoding
    # graph generation simpler. By default the value is 1e-300
    false_positive_rate: float = DEM_MIN_PROBABILITY

    def __post_init__(self) -> None:
        self.sanity_check()

    def of(
        circuit: stim.Circuit,
        false_positive_rate: float = DEM_MIN_PROBABILITY,
    ) -> "HeraldedDetectorErrorModel":
        return HeraldedDetectorErrorModel(
            ref_circuit=RefCircuit.of(circuit), false_positive_rate=false_positive_rate
        )

    def sanity_check(self) -> None:
        # check basic type
        assert isinstance(
            self.ref_circuit, RefCircuit
        ), "ref_circuit must be a RefCircuit"
        assert isinstance(
            self.false_positive_rate, float
        ), "false_positive_rate must be a float"
        # each heralded error is either not detected or uniquely detected
        for heralded_rec in self.heralded_measurements:
            assert (
                len(self.ref_circuit.rec_to_detectors[heralded_rec]) <= 1
            ), f"abs[{heralded_rec.abs_index(self.ref_circuit)}] is detected by multiple detectors"
            for detector in self.ref_circuit.rec_to_detectors[heralded_rec]:
                assert len(detector.targets) == 1, (
                    f"detector[{self.ref_circuit.detector_to_index[detector]}] detects multiple recs; "
                    + "we require that the detector of a heralded error must only detect one rec"
                )

    @functools.cached_property
    def heralded_instructions(self) -> tuple[RefInstruction, ...]:
        return tuple(
            instruction
            for instruction in self.ref_circuit
            if is_heralded_error(instruction)
        )

    @functools.cached_property
    def heralded_measurements(self) -> tuple[RefRec, ...]:
        return tuple(
            rec
            for instruction in self.ref_circuit
            if is_heralded_error(instruction)
            for rec in instruction.recs
        )

    @functools.cached_property
    def detected_heralded_measurements(self) -> tuple[RefRec, ...]:
        return tuple(
            rec
            for rec in self.heralded_measurements
            if self.ref_circuit.rec_to_detectors[rec]
        )

    @functools.cached_property
    def undetected_heralded_measurements(self) -> tuple[RefRec, ...]:
        return tuple(
            rec
            for rec in self.heralded_measurements
            if not self.ref_circuit.rec_to_detectors[rec]
        )

    @functools.cached_property
    def heralded_detectors(self) -> tuple[RefDetector | None, ...]:
        heralded_measurements = frozenset(self.heralded_measurements)
        return tuple(
            detector
            for detector in self.ref_circuit.detectors
            if set(detector.targets) & heralded_measurements
        )

    @functools.cached_property
    def skeleton_circuit(self) -> RefCircuit:
        """
        The skeleton circuit is a circuit where all the heralded errors are not triggered.
        There will be still some false positive rate remaining to make sure all the possible
        hyperedges still exist in the decoding hypergraph.
        """
        new_instructions = list(self.ref_circuit)
        deleting_indices: list[int] = []
        # first change instruction:
        #     HERALDED_ERASE -> DEPOLARIZE1(false_positive_rate)
        #     HERALDED_PAULI_CHANNEL_1 -> PAULI_CHANNEL_1(...)
        for instruction in self.heralded_instructions:
            instruction_index = self.ref_circuit.instruction_to_index[instruction]
            noise_instruction = heralded_instruction_to_noise_instruction(instruction)
            if noise_instruction is None:
                deleting_indices.append(instruction_index)
                continue
            tiny_noise_instruction = RefInstruction(
                name=noise_instruction.name,
                targets=noise_instruction.targets,
                gate_args=tuple(
                    dem_probability(p * self.false_positive_rate)
                    for p in noise_instruction.gate_args
                ),
            )
            new_instructions[instruction_index] = tiny_noise_instruction
        # then delete detectors of the heralded errors
        for detector in self.heralded_detectors:
            if detector is not None:
                deleting_indices.append(self.ref_circuit.instruction_to_index[detector])
        assert len(set(deleting_indices)) == len(deleting_indices), "bug: duplicate"
        for index in sorted(deleting_indices, reverse=True):
            del new_instructions[index]
        return RefCircuit.of(new_instructions)

    @functools.cached_property
    def skeleton_dem(self) -> RefDetectorErrorModel:
        """
        construct a dem whose detector id corresponds to the detectors of the original circuit
        instead of the skeleton circuit
        """
        ref_dem = self.skeleton_circuit.ref_dem
        # let the dem refer to self.ref_circuit instead
        return RefDetectorErrorModel(
            instructions=ref_dem.instructions, ref_circuit=self.ref_circuit
        )

    @functools.cached_property
    def heralded_dems(
        self,
    ) -> frozendict[RefDetector, RefDetectorErrorModel]:
        skeleton_hyperedges = self.skeleton_dem.hyperedges
        ref_dems: dict[RefDetector, RefDetectorErrorModel] = {}
        for detector in self.heralded_detectors:
            if detector is None:
                continue
            ref_rec = detector.targets[0]
            heralded_instruction = ref_rec.instruction
            all_noise_instruction = heralded_instruction_to_noise_instruction(
                heralded_instruction
            )
            if all_noise_instruction is None:
                continue
            assert isinstance(ref_rec, RefRec)
            # remove all the noise channels except the heralded error
            circuit_no_noise = self.ref_circuit.remove_noise_channels(
                keeping={ref_rec.instruction}
            )
            new_circuit_instructions = list(circuit_no_noise)
            # change the heralded error to the noise channel when it happens
            assert (
                len(all_noise_instruction.targets)
                == heralded_instruction.num_measurements
            ), "the following code assumes target has a heralding measurement"
            new_circuit_instructions[
                circuit_no_noise.instruction_to_index[heralded_instruction]
            ] = RefInstruction(
                name=all_noise_instruction.name,
                targets=(all_noise_instruction.targets[ref_rec.bias],),
                gate_args=all_noise_instruction.gate_args,
            )
            new_circuit = RefCircuit.of(new_circuit_instructions)
            heralded_dem = RefDetectorErrorModel(
                instructions=new_circuit.ref_dem.instructions,
                ref_circuit=self.ref_circuit,
            )
            if not heralded_dem.hyperedges:
                # if there is no hyperedge, we don't need this detector at all
                continue
            for hyperedge in heralded_dem.hyperedges:
                assert hyperedge in skeleton_hyperedges, (
                    "bug: the skeleton graph doesn't have the hyperedge, "
                    + "this might causes issue when constructing decoders"
                )
            ref_dems[detector] = heralded_dem

        return frozendict(ref_dems)

    def __str__(self) -> str:
        result = "HeraldedDetectorErrorModel:"
        result += "\n    skeleton hypergraph:"
        for hyperedge, p in self.skeleton_dem.hyperedges.items():
            result += (
                f"\n        {', '.join([f'D{v}' for v in sorted(hyperedge)])}: {p}"
            )
        for detector, ref_dem in self.heralded_dems.items():
            result += f"\n    heralded hypergraph on D{self.ref_circuit.detector_to_index[detector]}:"
            for hyperedge, p in ref_dem.hyperedges.items():
                result += (
                    f"\n        {', '.join([f'D{v}' for v in sorted(hyperedge)])}: {p}"
                )
        return result


def add_herald_detectors(circuit: stim.Circuit) -> stim.Circuit:
    """
    Add detectors for heralded errors to the circuit, if they do not present in the original circuit.
    Note that the circuit will be fully expanded after this function. The heralded detectors added
    will be following each heralded error instruction.
    """
    ref_circuit = RefCircuit.of(circuit)
    heralded_instructions = tuple(
        instruction for instruction in ref_circuit if is_heralded_error(instruction)
    )
    new_instructions = list(ref_circuit)
    for heralded_instruction in reversed(heralded_instructions):
        for rec in reversed(heralded_instruction.recs):
            if not ref_circuit.rec_to_detectors[rec]:
                new_instructions.insert(
                    ref_circuit.instruction_to_index[heralded_instruction] + 1,
                    RefInstruction(
                        name="DETECTOR",
                        targets=(rec,),
                    ),
                )
    return RefCircuit.of(new_instructions).circuit()


def is_heralded_error(instruction: RefInstruction) -> bool:
    if instruction.name in ["HERALDED_ERASE", "HERALDED_PAULI_CHANNEL_1"]:
        return True
    if "HERALDED" in instruction.name:
        raise NotImplementedError(
            f"Instruction {instruction.name} has 'HERALDED' in its name but is not implemented yet."
        )
    return False


def heralded_instruction_to_noise_instruction(
    instruction: RefInstruction,
) -> RefInstruction | None:
    if instruction.name == "HERALDED_ERASE":
        heralded_probability = instruction.gate_args[0]
        if heralded_probability == 0:
            return None
        return RefInstruction(
            name="DEPOLARIZE1",
            targets=instruction.targets,
            gate_args=(0.75,),
        )
    elif instruction.name == "HERALDED_PAULI_CHANNEL_1":
        _pI, pX, pY, pZ = instruction.gate_args
        p_sum = pX + pY + pZ
        if p_sum == 0:
            return None
        return RefInstruction(
            name="PAULI_CHANNEL_1",
            targets=instruction.targets,
            gate_args=(pX / p_sum, pY / p_sum, pZ / p_sum),
        )
    return None
