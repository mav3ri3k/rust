// Validate instruction sequence for
// - function body
// - subsequence in control instructions like block, loop, if
// - initilization sequences for globals and table elems
//
// Algorithm for validating instructions:
//   https://webassembly.github.io/spec/core/appendix/algorithm.html#algo-valid

use crate::error::{ErrorKind, Ordinal, Result};
use crate::Context as OuterContext;
use std::mem;
use wain_ast::source::Source;
use wain_ast::*;

#[derive(Default)]
struct CtrlFrame {
    height: usize,
    source_offset: usize,
    // Unreachability of current instruction sequence
    unreachable: bool,
    // True when value type at top of stack is unknown. This unknown type value appears when
    // polymorphic instructions follow unreachable. Since operands of them are polymorphic,
    // their types are unknown. Currently polymorphic instructions are only `select` and `drop`.
    // This is different from original algorithm in appendix of Wasm spec, but it's still OK to
    // manage unknown type value with flag because there is at most 1 unknown type on the operand
    // stack. (#52)
    has_unknown_type: bool,
}

// https://webassembly.github.io/spec/core/valid/conventions.html#context
#[allow(explicit_outlives_requirements)]
struct FuncBodyContext<'outer, 'module: 'outer, 'source: 'module, S: Source> {
    current_op: &'static str,
    current_offset: usize,
    outer: &'outer OuterContext<'module, 'source, S>,
    // Types on stack to check operands of instructions such as unreachable, br, table_br
    op_stack: Vec<ValType>,
    // Index of current control frame
    current_frame: CtrlFrame,
    // Label stack to verify jump instructions. None means no type for the label
    label_stack: Vec<Option<ValType>>,
    // The list of locals declared in the current function (including parameters), represented by their value type.
    // It's empty when validating outside function.
    params: &'outer [ValType],
    locals: &'outer [ValType],
    // Return type of the current function if exists
    ret_ty: Option<ValType>,
}

impl<'outer, 'm, 's, S: Source> FuncBodyContext<'outer, 'm, 's, S> {
    fn error<T>(&self, kind: ErrorKind) -> Result<T, S> {
        self.outer.error(kind, self.current_op, self.current_offset)
    }

    fn current_frame_empty(&self) -> Result<bool, S> {
        if self.op_stack.len() > self.current_frame.height {
            return Ok(false);
        }

        if self.current_frame.unreachable {
            // Reach top of current control frame, but it's ok when unreachable. For example,
            //
            //   unreachable i32.add
            //
            // should be valid. In the case operands of i32.add are not checked. For example,
            //
            //   unreachable (i64.const 0) i32.add
            //
            // should be invalid. In the case one operand of i32.add is i64. To archive this check,
            // popping operand stack has trick. Unknown type is simply ignored on check.
            return Ok(true);
        }

        // When not unreachable and stack hits bottom of current control frame, this instruction
        // sequence is invalid because some value should have been pushed on stack
        self.error(ErrorKind::CtrlFrameEmpty {
            op: self.current_op,
            frame_start: self.current_frame.source_offset,
            idx_in_op_stack: self.current_frame.height,
        })
    }

    fn ensure_op_stack_top(&mut self, expected: ValType) -> Result<(), S> {
        if self.current_frame_empty()? {
            // When type of top value is unknown, replace it with expected type
            self.current_frame.has_unknown_type = false;
            self.op_stack.push(expected);
            return Ok(());
        }

        let actual = self.op_stack[self.op_stack.len() - 1];

        if actual != expected {
            self.error(ErrorKind::TypeMismatch {
                expected: Some(expected),
                actual: Some(actual),
            })
        } else {
            Ok(())
        }
    }

    fn pop_op_stack(&mut self, expected: ValType) -> Result<(), S> {
        self.ensure_op_stack_top(expected)?;
        self.op_stack.pop();
        Ok(())
    }

    fn drop_op_stack(&mut self) -> Result<(), S> {
        if self.current_frame_empty()? {
            // When type of top value is unknown simply unset the flag
            self.current_frame.has_unknown_type = false;
        } else {
            self.op_stack.pop();
        }
        Ok(())
    }

    fn select_op_stack(&mut self) -> Result<(), S> {
        if self.current_frame_empty()? {
            // When stack is empty due to unreachable. In the case type of operand is unknown
            // because 'select' instruction is value-polymorphic. Set unknown flag instead of
            // pushing to stack
            self.current_frame.has_unknown_type = true;
            return Ok(());
        }

        let first = self.op_stack.pop().unwrap();

        // Check the first and second values are the same type
        self.ensure_op_stack_top(first)
    }

    fn push_control_frame(&mut self, source_offset: usize) -> CtrlFrame {
        let new = CtrlFrame {
            height: self.op_stack.len(),
            source_offset,
            unreachable: false,
            has_unknown_type: false,
        };
        mem::replace(&mut self.current_frame, new)
    }

    fn pop_control_frame(&mut self, prev: CtrlFrame, ty: Option<ValType>) -> Result<(), S> {
        // control frame top is validated by pop_op_stack
        if let Some(ty) = ty {
            self.pop_op_stack(ty)?;
        }
        let expected = self.current_frame.height;
        // When unknown flag is set, it means that there is a value of unknown type at top of stack
        // It adds 1 to length of `op_stack`
        let actual = self.op_stack.len() + self.current_frame.has_unknown_type as usize;
        assert!(expected <= actual);
        if expected != actual {
            return self.error(ErrorKind::InvalidStackDepth {
                expected,
                actual,
                remaining: format!("{:?}", &self.op_stack[expected..]),
            });
        }
        self.current_frame = prev;
        Ok(())
    }

    fn mark_unreachable(&mut self, stack_top: Option<ValType>) -> Result<(), S> {
        if let Some(ty) = stack_top {
            self.pop_op_stack(ty)?;
        }
        assert!(self.op_stack.len() >= self.current_frame.height);
        self.op_stack.truncate(self.current_frame.height);
        self.current_frame.unreachable = true;
        Ok(())
    }

    fn pop_label_stack(&mut self) {
        assert!(!self.label_stack.is_empty());
        self.label_stack.pop();
    }

    fn validate_label_idx(&self, idx: u32) -> Result<Option<ValType>, S> {
        let len = self.label_stack.len();
        if (idx as usize) >= len {
            return self.error(ErrorKind::IndexOutOfBounds {
                idx,
                upper: len,
                what: "label",
            });
        }
        let ty = self.label_stack[len - 1 - (idx as usize)];
        Ok(ty)
    }

    fn validate_local_idx(&self, idx: u32) -> Result<ValType, S> {
        let uidx = idx as usize;
        if let Some(ty) = self.params.get(uidx) {
            return Ok(*ty);
        }

        if let Some(ty) = self.locals.get(uidx - self.params.len()) {
            Ok(*ty)
        } else {
            self.error(ErrorKind::IndexOutOfBounds {
                idx,
                upper: self.locals.len(),
                what: "local variable",
            })
            .map_err(|e| {
                e.update_msg(format!(
                    "access to {} local variable at {}",
                    Ordinal(uidx - self.params.len()),
                    self.current_op
                ))
            })
        }
    }

    fn validate_memarg(&self, mem: &Mem, bits: u32) -> Result<(), S> {
        self.outer.memory_from_idx(0, self.current_op, self.current_offset)?;
        // The alignment must not be larger than the bit width of t divided by 8.
        if mem.align > (bits / 8).trailing_zeros() {
            return self.error(ErrorKind::TooLargeAlign { align: mem.align, bits });
        }
        Ok(())
    }

    fn validate_load(&mut self, mem: &Mem, bits: u32, ty: ValType) -> Result<(), S> {
        self.validate_memarg(mem, bits)?;
        self.pop_op_stack(ValType::I32)?; // load address
        self.op_stack.push(ty);
        Ok(())
    }

    fn validate_store(&mut self, mem: &Mem, bits: u32, ty: ValType) -> Result<(), S> {
        self.validate_memarg(mem, bits)?;
        self.pop_op_stack(ty)?; // value to store
        self.pop_op_stack(ValType::I32)?; // store address
        Ok(())
    }

    fn validate_convert(&mut self, from: ValType, to: ValType) -> Result<(), S> {
        self.pop_op_stack(from)?;
        self.op_stack.push(to);
        Ok(())
    }
}

pub(crate) fn validate_func_body<'outer, S: Source>(
    body: &'outer [Instruction],
    func_ty: &'outer FuncType,
    locals: &'outer [ValType],
    outer: &'outer OuterContext<'_, '_, S>,
    start: usize,
) -> Result<(), S> {
    // Note: FuncType already validated func_ty has at most one result type
    // This assumes a function can have only one return value
    let ret_ty = func_ty.results.first().copied();
    let mut ctx = FuncBodyContext {
        current_op: "",
        current_offset: start,
        outer,
        op_stack: vec![],
        label_stack: vec![ret_ty],
        current_frame: CtrlFrame {
            height: 0,
            source_offset: start,
            unreachable: false,
            has_unknown_type: false,
        },
        params: &func_ty.params,
        locals,
        ret_ty,
    };

    body.validate(&mut ctx)?;

    // No value must not remain in current frame after popping return values
    ctx.current_op = "function return";
    ctx.current_offset = start;
    ctx.pop_control_frame(Default::default(), ret_ty)
}

trait ValidateInsnSeq<'outer, 'm, 's, S: Source> {
    fn validate(&self, ctx: &mut FuncBodyContext<'outer, 'm, 's, S>) -> Result<(), S>;
}

impl<'s, 'm, 'outer, S: Source, V: ValidateInsnSeq<'outer, 'm, 's, S>> ValidateInsnSeq<'outer, 'm, 's, S> for [V] {
    fn validate(&self, ctx: &mut FuncBodyContext<'outer, 'm, 's, S>) -> Result<(), S> {
        self.iter().try_for_each(|insn| insn.validate(ctx))
    }
}

// https://webassembly.github.io/spec/core/valid/instructions.html#instruction-sequences
impl<'outer, 'm, 's, S: Source> ValidateInsnSeq<'outer, 'm, 's, S> for Instruction {
    fn validate(&self, ctx: &mut FuncBodyContext<'outer, 'm, 's, S>) -> Result<(), S> {
        ctx.current_op = self.kind.name();
        ctx.current_offset = self.start;
        let start = self.start;
        use InsnKind::*;
        match &self.kind {
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-block
            Block { ty, body } => {
                let saved = ctx.push_control_frame(start);
                ctx.label_stack.push(*ty);
                body.validate(ctx)?;
                ctx.pop_label_stack();
                ctx.pop_control_frame(saved, *ty)?;
                if let Some(ty) = *ty {
                    ctx.op_stack.push(ty);
                }
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-loop
            Loop { ty, body } => {
                let saved = ctx.push_control_frame(start);
                ctx.label_stack.push(None);
                body.validate(ctx)?;
                ctx.pop_label_stack();
                ctx.pop_control_frame(saved, *ty)?;
                if let Some(ty) = *ty {
                    ctx.op_stack.push(ty);
                }
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-if
            If {
                ty,
                then_body,
                else_body,
            } => {
                // Condition
                ctx.pop_op_stack(ValType::I32)?;
                ctx.label_stack.push(*ty);

                let saved = ctx.push_control_frame(start);
                then_body.validate(ctx)?;
                ctx.pop_control_frame(saved, *ty)?;

                let saved = ctx.push_control_frame(start);
                else_body.validate(ctx)?;
                ctx.pop_control_frame(saved, *ty)?;

                ctx.pop_label_stack();
                if let Some(ty) = *ty {
                    ctx.op_stack.push(ty);
                }
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-unreachable
            Unreachable => ctx.mark_unreachable(None)?,
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-nop
            Nop => {}
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-br
            Br(labelidx) => {
                let ty = ctx.validate_label_idx(*labelidx)?;
                ctx.mark_unreachable(ty)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-br-if
            BrIf(labelidx) => {
                // Condition
                ctx.pop_op_stack(ValType::I32)?;
                if let Some(ty) = ctx.validate_label_idx(*labelidx)? {
                    ctx.ensure_op_stack_top(ty)?;
                }
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-br-table
            BrTable { labels, default_label } => {
                ctx.pop_op_stack(ValType::I32)?;
                let expected = ctx.validate_label_idx(*default_label)?;
                for (i, idx) in labels.iter().enumerate() {
                    let actual = ctx.validate_label_idx(*idx)?;
                    if expected != actual {
                        return ctx
                            .error(ErrorKind::TypeMismatch { expected, actual })
                            .map_err(|e| e.update_msg(format!("{} label {} at {}", Ordinal(i), idx, ctx.current_op)));
                    }
                }
                ctx.mark_unreachable(expected)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-return
            Return => {
                ctx.mark_unreachable(ctx.ret_ty)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-call
            Call(funcidx) => {
                let func = ctx.outer.func_from_idx(*funcidx, ctx.current_op, start)?;
                // func.idx might be invalid when the callee is not validated yet (#39)
                let fty = ctx.outer.type_from_idx(func.idx, "callee at call instruction", start)?;
                // Pop extracts parameters in reverse order
                for (i, ty) in fty.params.iter().enumerate().rev() {
                    ctx.pop_op_stack(*ty)
                        .map_err(|e| e.update_msg(format!("{} parameter at call", Ordinal(i))))?;
                }
                ctx.op_stack.extend_from_slice(&fty.results);
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-call-indirect
            CallIndirect(typeidx) => {
                ctx.outer.table_from_idx(0, ctx.current_op, start)?;
                // Check table index
                ctx.pop_op_stack(ValType::I32)?;
                let fty = ctx.outer.type_from_idx(*typeidx, ctx.current_op, start)?;
                // Pop extracts parameters in reverse order
                for (i, ty) in fty.params.iter().enumerate().rev() {
                    ctx.pop_op_stack(*ty)
                        .map_err(|e| e.update_msg(format!("{} parameter at call.indirect", Ordinal(i))))?;
                }
                ctx.op_stack.extend_from_slice(&fty.results);
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-drop
            Drop => ctx.drop_op_stack()?,
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-select
            Select => {
                ctx.pop_op_stack(ValType::I32)?;
                ctx.select_op_stack()?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-local-get
            LocalGet(localidx) => {
                let ty = ctx.validate_local_idx(*localidx)?;
                ctx.op_stack.push(ty);
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-local-set
            LocalSet(localidx) => {
                let ty = ctx.validate_local_idx(*localidx)?;
                ctx.pop_op_stack(ty)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-local-tee
            LocalTee(localidx) => {
                let ty = ctx.validate_local_idx(*localidx)?;
                // pop and push the same value
                ctx.ensure_op_stack_top(ty)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-global-get
            GlobalGet(globalidx) => {
                let global = ctx.outer.global_from_idx(*globalidx, ctx.current_op, start)?;
                ctx.op_stack.push(global.ty);
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-global-set
            GlobalSet(globalidx) => {
                let global = ctx.outer.global_from_idx(*globalidx, ctx.current_op, start)?;
                let ty = global.ty;
                if !global.mutable {
                    return ctx.error(ErrorKind::SetImmutableGlobal { ty, idx: *globalidx });
                }
                ctx.pop_op_stack(ty)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-load
            I32Load(mem) => ctx.validate_load(mem, 32, ValType::I32)?,
            I64Load(mem) => ctx.validate_load(mem, 64, ValType::I64)?,
            F32Load(mem) => ctx.validate_load(mem, 32, ValType::F32)?,
            F64Load(mem) => ctx.validate_load(mem, 64, ValType::F64)?,
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-loadn
            I32Load8S(mem) => ctx.validate_load(mem, 8, ValType::I32)?,
            I32Load8U(mem) => ctx.validate_load(mem, 8, ValType::I32)?,
            I32Load16S(mem) => ctx.validate_load(mem, 16, ValType::I32)?,
            I32Load16U(mem) => ctx.validate_load(mem, 16, ValType::I32)?,
            I64Load8S(mem) => ctx.validate_load(mem, 8, ValType::I64)?,
            I64Load8U(mem) => ctx.validate_load(mem, 8, ValType::I64)?,
            I64Load16S(mem) => ctx.validate_load(mem, 16, ValType::I64)?,
            I64Load16U(mem) => ctx.validate_load(mem, 16, ValType::I64)?,
            I64Load32S(mem) => ctx.validate_load(mem, 32, ValType::I64)?,
            I64Load32U(mem) => ctx.validate_load(mem, 32, ValType::I64)?,
            // https://webassembly.github.io/spec/core/valid/instructions.html#id16
            I32Store(mem) => ctx.validate_store(mem, 32, ValType::I32)?,
            I64Store(mem) => ctx.validate_store(mem, 64, ValType::I64)?,
            F32Store(mem) => ctx.validate_store(mem, 32, ValType::F32)?,
            F64Store(mem) => ctx.validate_store(mem, 64, ValType::F64)?,
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-storen
            I32Store8(mem) => ctx.validate_store(mem, 8, ValType::I32)?,
            I32Store16(mem) => ctx.validate_store(mem, 16, ValType::I32)?,
            I64Store8(mem) => ctx.validate_store(mem, 8, ValType::I64)?,
            I64Store16(mem) => ctx.validate_store(mem, 16, ValType::I64)?,
            I64Store32(mem) => ctx.validate_store(mem, 32, ValType::I64)?,
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-memory-size
            MemorySize => {
                if ctx.outer.module.memories.is_empty() {
                    return ctx.error(ErrorKind::MemoryIsNotDefined);
                }
                ctx.op_stack.push(ValType::I32);
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-memory-grow
            MemoryGrow => {
                if ctx.outer.module.memories.is_empty() {
                    return ctx.error(ErrorKind::MemoryIsNotDefined);
                }
                // pop i32 and push i32
                ctx.ensure_op_stack_top(ValType::I32)?;
            }
            I32Const(_) => ctx.op_stack.push(ValType::I32),
            I64Const(_) => ctx.op_stack.push(ValType::I64),
            F32Const(_) => ctx.op_stack.push(ValType::F32),
            F64Const(_) => ctx.op_stack.push(ValType::F64),
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-unop
            // [t] -> [t]
            I32Clz | I32Ctz | I32Popcnt => ctx.ensure_op_stack_top(ValType::I32)?,
            I64Clz | I64Ctz | I64Popcnt => ctx.ensure_op_stack_top(ValType::I64)?,
            F32Abs | F32Neg | F32Ceil | F32Floor | F32Trunc | F32Nearest | F32Sqrt => {
                ctx.ensure_op_stack_top(ValType::F32)?;
            }
            F64Abs | F64Neg | F64Ceil | F64Floor | F64Trunc | F64Nearest | F64Sqrt => {
                ctx.ensure_op_stack_top(ValType::F64)?;
            }
            I32Extend8S | I32Extend16S => {
                ctx.ensure_op_stack_top(ValType::I32)?;
            }
            I64Extend8S | I64Extend16S | I64Extend32S => {
                ctx.ensure_op_stack_top(ValType::I64)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-binop
            // [t t] -> [t]
            I32Add | I32Sub | I32Mul | I32DivS | I32DivU | I32RemS | I32RemU | I32And | I32Or | I32Xor | I32Shl
            | I32ShrS | I32ShrU | I32Rotl | I32Rotr => {
                ctx.pop_op_stack(ValType::I32)?;
                ctx.ensure_op_stack_top(ValType::I32)?;
            }
            I64Add | I64Sub | I64Mul | I64DivS | I64DivU | I64RemS | I64RemU | I64And | I64Or | I64Xor | I64Shl
            | I64ShrS | I64ShrU | I64Rotl | I64Rotr => {
                ctx.pop_op_stack(ValType::I64)?;
                ctx.ensure_op_stack_top(ValType::I64)?;
            }
            F32Add | F32Sub | F32Mul | F32Div | F32Min | F32Max | F32Copysign => {
                ctx.pop_op_stack(ValType::F32)?;
                ctx.ensure_op_stack_top(ValType::F32)?;
            }
            F64Add | F64Sub | F64Mul | F64Div | F64Min | F64Max | F64Copysign => {
                ctx.pop_op_stack(ValType::F64)?;
                ctx.ensure_op_stack_top(ValType::F64)?;
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-testop
            // [t] -> [i32]
            I32Eqz => ctx.ensure_op_stack_top(ValType::I32)?,
            I64Eqz => {
                ctx.pop_op_stack(ValType::I64)?;
                ctx.op_stack.push(ValType::I32);
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-relop
            // [t t] -> [i32]
            I32Eq | I32Ne | I32LtS | I32LtU | I32GtS | I32GtU | I32LeS | I32LeU | I32GeS | I32GeU => {
                ctx.pop_op_stack(ValType::I32)?;
                ctx.ensure_op_stack_top(ValType::I32)?;
            }
            I64Eq | I64Ne | I64LtS | I64LtU | I64GtS | I64GtU | I64LeS | I64LeU | I64GeS | I64GeU => {
                ctx.pop_op_stack(ValType::I64)?;
                ctx.pop_op_stack(ValType::I64)?;
                ctx.op_stack.push(ValType::I32);
            }
            F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge => {
                ctx.pop_op_stack(ValType::F32)?;
                ctx.pop_op_stack(ValType::F32)?;
                ctx.op_stack.push(ValType::I32);
            }
            F64Eq | F64Ne | F64Lt | F64Gt | F64Le | F64Ge => {
                ctx.pop_op_stack(ValType::F64)?;
                ctx.pop_op_stack(ValType::F64)?;
                ctx.op_stack.push(ValType::I32);
            }
            // https://webassembly.github.io/spec/core/valid/instructions.html#valid-cvtop
            // [t1] -> [t2]
            I32WrapI64 => ctx.validate_convert(ValType::I64, ValType::I32)?,
            I32TruncF32S | I32TruncF32U | I32ReinterpretF32 => ctx.validate_convert(ValType::F32, ValType::I32)?,
            I32TruncF64S | I32TruncF64U => ctx.validate_convert(ValType::F64, ValType::I32)?,
            I64ExtendI32S | I64ExtendI32U => ctx.validate_convert(ValType::I32, ValType::I64)?,
            I64TruncF32S | I64TruncF32U => ctx.validate_convert(ValType::F32, ValType::I64)?,
            I64TruncF64S | I64TruncF64U | I64ReinterpretF64 => ctx.validate_convert(ValType::F64, ValType::I64)?,
            F32ConvertI32S | F32ConvertI32U | F32ReinterpretI32 => ctx.validate_convert(ValType::I32, ValType::F32)?,
            F32ConvertI64S | F32ConvertI64U => ctx.validate_convert(ValType::I64, ValType::F32)?,
            F32DemoteF64 => ctx.validate_convert(ValType::F64, ValType::F32)?,
            F64ConvertI32S | F64ConvertI32U => ctx.validate_convert(ValType::I32, ValType::F64)?,
            F64ConvertI64S | F64ConvertI64U | F64ReinterpretI64 => ctx.validate_convert(ValType::I64, ValType::F64)?,
            F64PromoteF32 => ctx.validate_convert(ValType::F32, ValType::F64)?,
        }
        Ok(())
    }
}

// https://webassembly.github.io/spec/core/valid/instructions.html#constant-expressions
pub(crate) fn validate_constant<S: Source>(
    insns: &[Instruction],
    ctx: &OuterContext<'_, '_, S>,
    expr_ty: ValType,
    when: &'static str,
    start: usize,
) -> Result<(), S> {
    match insns.len() {
        0 => return ctx.error(ErrorKind::NoInstructionForConstant, when, start),
        1 => {}
        len => return ctx.error(ErrorKind::TooManyInstructionForConstant(len), when, start),
    }

    use InsnKind::*;
    let insn = &insns[0];
    let name = insn.kind.name();
    let ty = match &insn.kind {
        GlobalGet(globalidx) => {
            // https://webassembly.github.io/spec/core/valid/instructions.html#constant-expressions
            // Contained global.get instructions are only allowed to refer to imported globals.
            //
            // https://webassembly.github.io/spec/core/valid/modules.html#valid-module
            // As formal manner, the context of globals is C', not C, where C' only contains imported globals.
            // Globals are validated under C'. It means only imported globals are visible while the validation.
            if *globalidx < ctx.num_import_globals as u32 {
                let global = ctx.module.globals.get(*globalidx as usize).unwrap();
                if global.mutable {
                    return ctx.error(ErrorKind::MutableForConstant(*globalidx), when, start);
                }
                global.ty
            } else {
                return ctx
                    .error(
                        ErrorKind::IndexOutOfBounds {
                            idx: *globalidx,
                            upper: ctx.num_import_globals,
                            what: "global variable read",
                        },
                        "",
                        insn.start,
                    )
                    .map_err(|e| e.update_msg(format!("constant expression in {} at {}", name, when)));
            }
        }
        I32Const(_) => ValType::I32,
        I64Const(_) => ValType::I64,
        F32Const(_) => ValType::F32,
        F64Const(_) => ValType::F64,
        _ => {
            return ctx
                .error(ErrorKind::NotConstantInstruction(name), "", insn.start)
                .map_err(|e| e.update_msg(format!("constant expression at {}", when)));
        }
    };
    if ty != expr_ty {
        ctx.error(
            ErrorKind::TypeMismatch {
                expected: Some(expr_ty),
                actual: Some(ty),
            },
            "",
            start,
        )
        .map_err(|e| e.update_msg(format!("type of constant expression at {}", when)))
    } else {
        Ok(())
    }
}
