#![allow(unused)]
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::{ExecutionEngine, JitFunction};
use inkwell::module::{self, Module};
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::types::PointerType;
use inkwell::values::BasicValue;
use inkwell::{types, AddressSpace, OptimizationLevel};

use std::error::Error;
use std::io::{self, Read};
use std::ops::Add;
use std::os::fd::AsRawFd;

type BfFunc = unsafe extern "C" fn() -> *mut u8;

const VALID_BF: [char; 8] = ['+', '-', '<', '>', '[', ']', ',', '.'];
const POLL_STDIN: bool = true;
//
fn count_chars_of_type(test_str: &str, match_to: char) -> (u64, u64) {
    let mut r = 0;
    let mut last_valid = 0;
    for c in test_str
        .chars()
        .enumerate()
        .filter(|c| VALID_BF.contains(&c.1))
    {
        if c.1 != match_to {
            return (r, last_valid + 1);
        }
        last_valid = c.0 as u64;
        r += 1;
    }
    (r, last_valid + 1)
}
struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    execution_engine: Option<ExecutionEngine<'ctx>>,
    target_machine: Option<TargetMachine>,
}
const STACK_SIZE: u32 = u16::MAX as u32;
impl<'ctx> CodeGen<'ctx> {
    fn new_jit(context: &'ctx Context) -> Self {
        let module = context.create_module("brain_fucked_jit");
        let exe = Some(
            module
                .create_jit_execution_engine(OptimizationLevel::None)
                .unwrap(),
        );
        Self {
            context,
            module,
            builder: context.create_builder(),
            execution_engine: exe,
            target_machine: None,
        }
    }
    fn new_comp(context: &'ctx Context) -> Self {
        let module = context.create_module("brain_fucked_comp");
        Target::initialize_all(&InitializationConfig::default());
        let target_triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&target_triple).unwrap();
        let target_machine = target
            .create_target_machine(
                &target_triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .unwrap();
        Self {
            context,
            module,
            builder: context.create_builder(),
            execution_engine: None,
            target_machine: Some(target_machine),
        }
    }
    fn gen_bf(&self, code_to_comp: String, do_free: bool) {
        // Define putchar and getchar function

        let putchar = self.module.add_function(
            "putchar",
            self.context
                .i32_type()
                .fn_type(&[self.context.i32_type().into()], false),
            None,
        );
        let getchar =
            self.module
                .add_function("getchar", self.context.i32_type().fn_type(&[], false), None);

        // Define main function
        let i64_type = self.context.ptr_type(AddressSpace::default());
        let fn_type = i64_type.fn_type(&[], false);
        let function = self.module.add_function("main", fn_type, None);
        let basic_block = self.context.append_basic_block(function, "entry");

        let stack_size_const = self.context.i32_type().const_int(STACK_SIZE as u64, false);
        let zero_consti8 = self.context.i8_type().const_int(0, false);
        let zero_consti32 = self.context.i32_type().const_int(0, false);

        let target_data = if let Some(exe) = self.execution_engine.as_ref() {
            exe.get_target_data()
        } else {
            &(self.target_machine.as_ref().unwrap().get_target_data())
        };
        let ptr_int_type = self.context.ptr_sized_int_type(&target_data, None);

        self.builder.position_at_end(basic_block);

        let mut loop_label_starts = Vec::new();
        let mut loop_label_ends = Vec::new();

        // i8* og_stack = malloc(STACK_SIZE)
        let og_stack = self
            .builder
            .build_malloc(self.context.i8_type().array_type(STACK_SIZE), "og_stack")
            .unwrap();
        // Memset stack to zero
        self.builder
            .build_memset(og_stack, 1, zero_consti8, stack_size_const)
            .unwrap();

        // diff_ptr = alloca(4)
        let diff_ptr = self
            .builder
            .build_alloca(self.context.i32_type(), "diff")
            .unwrap();
        // *diff_ptr = 0
        self.builder.build_store(diff_ptr, zero_consti32).unwrap();

        let stack_ptr_ptr = self
            .builder
            .build_alloca(self.context.ptr_type(0.into()), "array")
            .unwrap();
        // *stack_ptr_pre = og_stack
        self.builder.build_store(stack_ptr_ptr, og_stack).unwrap();

        let deref_stack = || {
            let stack_ptr = self
                .builder
                .build_load(
                    self.context.ptr_type(AddressSpace::default()),
                    stack_ptr_ptr,
                    "array_deref",
                )
                .unwrap()
                .into_pointer_value();
            let stack_val = self
                .builder
                .build_load(self.context.i8_type(), stack_ptr, "stack_deref")
                .unwrap()
                .into_int_value();
            (stack_ptr, stack_val)
        };
        let mut index = 0;
        while index < code_to_comp.len() {
            let chr = &code_to_comp[index..].chars().into_iter().next().unwrap();
            match chr {
                '+' | '-' => {
                    let amount = count_chars_of_type(&code_to_comp[index..], *chr);
                    index += amount.1 as usize;
                    let amount_const = self
                        .context
                        .i8_type()
                        .const_int(amount.0 % 256, false)
                        .as_basic_value_enum()
                        .into_int_value();
                    let (stack_ptr, current_data_ptr_val) = deref_stack();
                    let inc_or_dec_v = if *chr == '+' {
                        self.builder
                            .build_int_add(current_data_ptr_val, amount_const, "added_temp")
                    } else {
                        self.builder
                            .build_int_sub(current_data_ptr_val, amount_const, "subed_temp")
                    }
                    .unwrap();

                    self.builder.build_store(stack_ptr, inc_or_dec_v).unwrap();
                }
                '<' | '>' => {
                    let amount = count_chars_of_type(&code_to_comp[index..], *chr);
                    index += amount.1 as usize;

                    // diff = *diff_pre
                    let diff = self
                        .builder
                        .build_load(self.context.i32_type(), diff_ptr, "loaded_diff")
                        .unwrap()
                        .into_int_value();
                    // i32 amount_const = amount
                    let amount_const = self
                        .context
                        .i32_type()
                        .const_int(amount.0, false)
                        .as_basic_value_enum()
                        .into_int_value();

                    // diff (+/-)= amount_const
                    let diff = if *chr == '>' {
                        self.builder
                            .build_int_add(diff, amount_const, "add_temp")
                            .unwrap()
                    } else {
                        self.builder
                            .build_int_sub(diff, amount_const, "sub_temp")
                            .unwrap()
                    };
                    // diff %= stack_size // Keep ptr in bounds
                    let diff = self
                        .builder
                        .build_int_unsigned_rem(diff, stack_size_const, "mod_temp")
                        .unwrap();
                    // *diff_ptr = diff
                    self.builder.build_store(diff_ptr, diff);
                    // Zero extend diff into a usize
                    let diff = self
                        .builder
                        .build_int_z_extend(diff, ptr_int_type, "zero_extended_diff")
                        .unwrap();
                    // new_stack = og_stack + diff
                    let new_stack = self
                        .builder
                        .build_int_add(
                            og_stack,
                            diff.const_to_pointer(self.context.ptr_type(AddressSpace::default())),
                            "new_stack",
                        )
                        .unwrap();
                    // *stack_ptr_pre = new_stack
                    self.builder.build_store(stack_ptr_ptr, new_stack);
                }
                '[' => {
                    index += 1;
                    let start = self.context.append_basic_block(function, "start_of_loop");
                    let end = self.context.append_basic_block(function, "end_of_loop");
                    loop_label_starts.push(start);
                    loop_label_ends.push(end);

                    let (_, current_data_ptr_val) = deref_stack();
                    let is_zero = self
                        .builder
                        .build_int_compare(
                            inkwell::IntPredicate::EQ,
                            current_data_ptr_val,
                            zero_consti8,
                            "is_zero",
                        )
                        .unwrap();
                    self.builder.build_conditional_branch(is_zero, end, start);

                    self.builder.position_at_end(start);
                }
                ']' => {
                    index += 1;
                    let start = loop_label_starts.pop().unwrap();
                    let end = loop_label_ends.pop().unwrap();
                    let (_, current_data_ptr_val) = deref_stack();

                    let is_zero = self
                        .builder
                        .build_int_compare(
                            inkwell::IntPredicate::EQ,
                            current_data_ptr_val,
                            zero_consti8,
                            "is_zero",
                        )
                        .unwrap();
                    self.builder.build_conditional_branch(is_zero, end, start);
                    self.builder.position_at_end(end);
                }
                '.' => {
                    index += 1;

                    let (_, current_data_ptr_val) = deref_stack();
                    let data = self
                        .builder
                        .build_int_z_extend(
                            current_data_ptr_val,
                            self.context.i32_type(),
                            "zero_extended_val",
                        )
                        .unwrap();

                    self.builder.build_call(putchar, &[data.into()], "_");
                }
                ',' => {
                    index += 1;

                    let received_char = self
                        .builder
                        .build_call(getchar, &[], "gotChar")
                        .unwrap()
                        .try_as_basic_value()
                        .left()
                        .unwrap()
                        .into_int_value();
                    let received_char = self.builder.build_int_truncate(
                        received_char,
                        self.context.i8_type(),
                        "gotChar",
                    ).unwrap();

                    let (stack_ptr, _) = deref_stack();
                    self.builder.build_store(stack_ptr, received_char);
                }
                _ => {
                    index += 1;
                }
            }
        }

        if do_free {
            self.builder.build_free(og_stack).unwrap();
            self.builder
                .build_return(Some(&self.context.i32_type().const_int(0, false)))
                .unwrap();
            return;
        }
        self.builder.build_return(Some(&og_stack)).unwrap();
    }

    pub fn jit_compile_bf(context: &'ctx Context, code_to_comp: String) {
        let obj = Self::new_jit(context);
        obj.gen_bf(code_to_comp, false);

        let main: JitFunction<'ctx, BfFunc> = unsafe {
            obj.execution_engine
                .as_ref()
                .unwrap()
                .get_function("main")
                .unwrap()
        };

        let x: *mut u8 = unsafe { main.call() };
    }
    pub fn compile_bf_to_file(context: &'ctx Context, file: String, code_to_comp: String) {
        let obj = Self::new_comp(context);
        obj.gen_bf(code_to_comp, true);

        let target_machine = obj.target_machine.as_ref().unwrap();
        target_machine
            .write_to_file(&obj.module, FileType::Object, file.as_ref())
            .unwrap();
    }
}

fn main() {
    let mut stdin = std::io::stdin();
    let mut bf_code = String::new();
    stdin.read_to_string(&mut bf_code);

    let mut args: Vec<String> = std::env::args().collect();
    // No args = jit
    if (args.len() == 1) {
        return CodeGen::jit_compile_bf(&Context::create(), bf_code);
    }
    // Otherwise the .o file to output to

    return CodeGen::compile_bf_to_file(&Context::create(), args[1].clone(), bf_code);
}
