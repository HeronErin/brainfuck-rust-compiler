# Simple rust brainfuck compiler / jit interpreter  

In order to install you need rust and llvm 17 installed.

## Building an executable

```bash
    cat helloworld.bf | cargo run out.o
    gcc out.o -o out
    chmod +x out
    ./out
```

## Running as a jit
Warning, running as a JIT the `,` character **will not** bind, and instead will return 0xFF instantly!
```bash
    cat helloworld.bf | cargo run
```

