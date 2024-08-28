extern crate llvm_sys as llvm;
#[allow(dead_code, unused)]
use std::ffi::CStr;
use std::mem;

use llvm::core::*;
use llvm::execution_engine::*;
use llvm::target::*;
use llvm::LLVMBasicBlock;
use llvm::LLVMBuilder;
const STACK_SIZE: u64 = 128;
const input: &str = "+++++ +++++             initialize counter (cell #0) to 10
    [                       use loop to set the next four cells to 70/100/30/10
        > +++++ ++              add  7 to cell #1
        > +++++ +++++           add 10 to cell #2 
        > +++                   add  3 to cell #3
        > +                     add  1 to cell #4
        <<<< -                  decrement counter (cell #0)
    ]                   
    > ++ .                  print 'H'
    > + .                   print 'e'
    +++++ ++ .              print 'l'
    .                       print 'l'
    +++ .                   print 'o'
    > ++ .                  print ' '
    << +++++ +++++ +++++ .  print 'W'
    > .                     print 'o'
    +++ .                   print 'r'
    ----- - .               print 'l'
    ----- --- .             print 'd'
    > + .                   print '!'
    > .                     print '\n'";

fn dcount(test_str: &str, match_to: char) -> u64 {
    let mut r = 0;
    for c in test_str.chars() {
        if c != match_to {
            break;
        }
        r += 1;
    }
    r
}

unsafe fn gen(builder: *mut LLVMBuilder, block: *mut LLVMBasicBlock) {
    let og_array = LLVMBuildMalloc(
        builder,
        LLVMArrayType2(LLVMInt32Type(), STACK_SIZE),
        "arr\0".as_ptr() as *const _,
    );

    // Fill with zeros
    LLVMBuildMemSet(
        builder,
        og_array,
        LLVMConstInt(LLVMInt8Type(), 0, 0),
        LLVMConstInt(LLVMInt32Type(), STACK_SIZE, 0),
        1,
    );
    let _array = LLVMBuildAlloca(
        builder,
        LLVMPointerType(LLVMInt32Type(), 0),
        "arr2_ptr\0".as_ptr() as *const _,
    );
    LLVMBuildStore(builder, og_array, _array);
    let mut array = LLVMBuildLoad2(
        builder,
        LLVMPointerType(LLVMInt32Type(), 0),
        _array,
        "arr2\0".as_ptr() as *const _,
    );
    let diff = LLVMBuildLoad2(
        builder,
        LLVMInt32Type(),
        array,
        "diff\0".as_ptr() as *const _,
    );

    let mut index = 0;
    while index < input.len() {
        match &input[index..index + 1] {
            "+" => {
                let amount = dcount(&input[index..], '+');
                index += amount as usize;

                // *array = *array + amount_of_pluses % 256
                LLVMBuildStore(
                    builder,
                    LLVMBuildAdd(
                        builder,
                        LLVMBuildLoad2(
                            builder,
                            LLVMInt8Type(),
                            array,
                            "add_temp\0".as_ptr() as *const _,
                        ),
                        LLVMConstInt(LLVMInt8Type(), amount % 256, 0),
                        "add_temp\0".as_ptr() as *const _,
                    ),
                    array,
                );
            }
            "-" => {
                let amount = dcount(&input[index..], '-');
                index += amount as usize;

                // *array = *array - amount_of_minuses % 256
                LLVMBuildStore(
                    builder,
                    LLVMBuildSub(
                        builder,
                        LLVMBuildLoad2(
                            builder,
                            LLVMInt8Type(),
                            array,
                            "sub_temp\0".as_ptr() as *const _,
                        ),
                        LLVMConstInt(LLVMInt8Type(), amount % 256, 0),
                        "sub_temp\0".as_ptr() as *const _,
                    ),
                    array,
                );
            }
            ">" => {
                let amount = dcount(&input[index..], '>');
                index += amount as usize;
                array = /*LLVMBuildAdd(
                    builder,
                    LLVMBuildURem(
                        builder,
                        */LLVMBuildAdd(
                            builder,
                            diff,
                            LLVMConstInt(LLVMInt32Type(), amount, 0),
                            "dtemp\0".as_ptr() as *const _,
                        ) /*,
                        LLVMConstInt(LLVMInt32Type(), STACK_SIZE, 0),
                        "mod_temp\0".as_ptr() as *const _,
                    ),
                    og_array,
                    "newarray\0".as_ptr() as *const _ 
                )*/;
            }
            _ => {
                index += 1;
            }
        }
    }

    LLVMBuildRet(builder, array);
}

fn main() {
    unsafe {
        // Set up a context, module and builder in that context.
        let context = LLVMContextCreate();
        LLVM_InitializeNativeTarget();
        let module =
            LLVMModuleCreateWithNameInContext(b"BrainFucked\0".as_ptr() as *const _, context);
        let builder = LLVMCreateBuilderInContext(context);

        let function = LLVMAddFunction(
            module,
            b"bf\0".as_ptr() as *const _,
            LLVMFunctionType(LLVMPointerType(LLVMInt8Type(), 0), [].as_mut_ptr(), 0, 0),
        );

        let bb = LLVMAppendBasicBlockInContext(context, function, b"entry\0".as_ptr() as *const _);

        LLVMPositionBuilderAtEnd(builder, bb);

        gen(builder, bb);

        LLVMDisposeBuilder(builder);
        LLVMDumpModule(module);

        LLVMLinkInMCJIT();

        LLVM_InitializeNativeAsmPrinter();

        // Build an execution engine.
        let ee = {
            let mut ee = mem::MaybeUninit::uninit();
            let mut err = mem::zeroed();

            // This moves ownership of the module into the execution engine.
            if LLVMCreateExecutionEngineForModule(ee.as_mut_ptr(), module, &mut err) != 0 {
                // In case of error, we must avoid using the uninitialized ExecutionEngineRef.
                assert!(!err.is_null());
                panic!(
                    "Failed to create execution engine: {:?}",
                    CStr::from_ptr(err)
                );
            }

            ee.assume_init()
        };
        let addr = LLVMGetFunctionAddress(ee, b"bf\0".as_ptr() as *const _);
        let f: extern "C" fn() -> *mut u8 = mem::transmute(addr);
        let a = f();
        let v = Vec::from_raw_parts(a, STACK_SIZE as usize, STACK_SIZE as usize);
        println!("Got {:?}", &v);
        println!("Addr {}", f as usize);

        LLVMDisposeExecutionEngine(ee);
        LLVMContextDispose(context);
    }
}
