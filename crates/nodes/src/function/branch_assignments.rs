use super::Program;

/// A pixel branch evaluates a shared assignment in the enclosing scope. An assignment
/// private to one arm stays there, including when two alternatives inside that arm use it.
pub(super) fn identify(program: &mut Program) {
    let instructions = &program.instructions;
    if !instructions.iter().any(|op| op.definition.id == "ifelse")
        || !instructions.iter().any(|op| op.definition.id == "set" || op.call.as_ref().is_some_and(|call| call.exports))
    {
        return;
    }

    let mut writes = Vec::with_capacity(instructions.len());
    for op in instructions {
        // The loop owns its assignments and refreshes their registers on each iteration.
        let assigns = op.definition.id != "while"
            && (op.definition.id == "set" || op.call.as_ref().is_some_and(|call| call.exports) || op.inputs.iter().any(|&input| writes[input]));
        writes.push(assigns);
    }

    let mut paths = vec![0_u8; instructions.len()];
    for branch in 0..instructions.len() {
        let op = &program.instructions[branch];
        let [_, yes, no] = op.inputs.as_slice() else { continue };
        if op.definition.id != "ifelse" || !(writes[*yes] || writes[*no]) {
            continue;
        }
        paths.fill(0);
        paths[program.output] = 4;
        paths[*yes] |= 1;
        paths[*no] |= 2;
        let mut shared = Vec::new();
        // Instructions follow their inputs. Reach a register from each arm and from
        // other consumers before deciding whether it belongs at this branch.
        for index in (0..=program.output).rev() {
            let path = paths[index];
            if path == 0 || index == branch || !writes[index] {
                continue;
            }
            if path.count_ones() > 1 {
                // Evaluate the whole shared expression in order. Pulling individual Sets
                // out of a shared sequence or conditional could execute a skipped write.
                shared.push(index);
            } else {
                for &input in &program.instructions[index].inputs {
                    paths[input] |= path;
                }
            }
        }
        shared.reverse();
        program.instructions[branch].control_registers = shared;
    }
}
