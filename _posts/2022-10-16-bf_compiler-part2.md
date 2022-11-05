---
layout: post
title:  "Compiling Brainfuck code - Part 2: A Single-Pass JIT Compiler"
date:   2022-10-22 12:00:00 -0300
---

<span class="ert">
    <abbr title="Estimated reading time">ERT</abbr>
    {% assign words_total = page.content | replace: '<script type="math/tex">', '' | replace: '<script type="math/tex; mode=display">', '' | replace: '</script>', '' | strip_html | number_of_words %}
    {% assign words_without_code = page.content | replace: '<pre class="highlight">', '<!--' | replace: '</pre>', '-->' | replace: '<script type="math/tex">', '' | replace: '<script type="math/tex; mode=display">', '' | replace: '</script>', '' | strip_html | number_of_words %}
    {% assign words_without_math = page.content | strip_html | number_of_words %}
    {% assign words_without_either = page.content | replace: '<pre class="highlight">', '<!--' | replace: '</pre>', '-->' | strip_html | number_of_words %}

    {% assign words_code = words_total | minus: words_without_code | divided_by: 2.0 %}
    {% assign words_math = words_total | minus: words_without_math | times: 2.0 %}
    {% assign words = words_without_either | plus: words_code | plus: words_math | round %}

    {% assign ert = words | divided_by:250 | at_least: 1 %}
    {{ ert }} minute{% if ert != 1 %}s{% endif %}
</span>

To-do's:
 - Talk more about the x86 ISA, like how the register nomenclature work.
 - Fix assembly throughout that lack stack alignment. Also mention functions
   prologue and epilogue, and the use `rbp`.
 - Introduce the concept of Runtime, and mention it throughout the post.
 - Check how many times I said "finally".
 - Explicitly mention and define "single-pass".
 - Mention nasm instead of gnu assembler.
 - When leaking mmap, talk a little about function pointers safety.

This is the second post of a blog post series where I will reproduce [Eli
Bendersky’s Adventures In JIT Compilation series][eli], but this time using the
Rust.

[eli]: https://eli.thegreenplace.net/2017/adventures-in-jit-compilation-part-1-an-interpreter
[rust]: https://www.rust-lang.org

The [previous part is here]({% post_url 2022-10-16-bf_compiler-part1 %}), where
we made an optimized brainfuck interpreter. In this part we will make a
brainfuck JIT compiler.

# The Interpreter Overhead

In the end of the previous post, I said that the implemented interpreter have
yet some optimizations opportunities to explore, but that there always would
be a significant performance overhead by its program loop.

To grasp exactly how big is this overhead, we can take a look at generated
assembly for the main loop of our interpreter, were the majority of the runtime
is spent.

In this case, I used [cargo-show-wasm] to get the x86 assembly for
`Program::run`, and after a lot clean up I got the following:

[cargo-show-wasm]: https://github.com/pacak/cargo-show-asm

```nasm
; 'program: loop (not including first iteration)
.PROGRAM:
    ; r13 is &self
    ; rcx is self.program_counter
    ; rdx is self.instructions.len()
    mov rcx, qword ptr [r13]
    mov rdx, qword ptr [r13 + 32]
    add rcx, 1
    mov qword ptr [r13], rcx
    cmp rdx, rcx                  ; break if instructions.len() == program_counter
    je .BREAK_PROGRAM             ; .

    ; rsi is pointer
    ; rax is self.instructios.ptr

    mov rsi, qword ptr [r13 + 8]
    mov rax, qword ptr [r13 + 16]

    ; `match instruction[pointer]` by using a jump table
    movzx edi, byte ptr [rax + rcx]     ; instr = instruction[program_counter]
    movsxd rdi, dword ptr [r12 + 4*rdi] ; rel_jump = jump_table[instr]
    add rdi, r12                        ; jump += rel_jump + &jump_table
    jmp rdi                             ; goto `jump`

; Increase (`+`)
.LBB8_91:
    add byte ptr [r13 + rsi + 40], 1
    jmp .PROGRAM

; Decrease (`-`)
.LBB8_90:
    add byte ptr [r13 + rsi + 40], -1
    jmp .PROGRAM

; MoveRight (`>`)
.LBB8_1:
    add esi, 1

    movzx eax, si
    mov ecx, eax
    shr ecx, 4
    imul ecx, ecx, 2237
    shr ecx, 22
    imul ecx, ecx, 30000
    sub eax, ecx
    movzx eax, ax
    mov qword ptr [r13 + 8], rax
    jmp .PROGRAM

; Etc..
```

I added some comments to the assembly to make it a little more clear what is
going on, but the main point here is the amount of opcodes that is used to
implement the interpreter loop compared to the opcodes used for the brainfuck
instructions. 

The `+` and `-` operations are being implemented by a single assembly
instruction (the `add byte ptr [r13 + rsi + 40], 1`), but after executing each
instruction it needs to do a jump and run everything in the `.PROGRAM` block,
who has 12 instructions. Instructions count is not a very precise performance
parameter, but this roughly means that the interpreter has an overhead of 13x
when running a `+` and `-` instructions.

This is a lesser problem for brainfuck instructions with bigger
implementations, like the `>` there, who has a very involved wrapping
calculation (could be greatly improved by the memory length being a power of
2), but the overhead is still very significant, almost 2x.

This roughly means that if we could eliminate this overhead, we can have
something between 2x and 13x of performance increase!

And to achieve this we will implement a JIT compiler.

# What is a compiler and a JIT compiler

A compiler is basically a program that transforms our high level code, easy to
write and understand, to a lower lever code, easy to execute. In this case, we
will compile our brainfuck code to *machine code*, the code of the instructions
executed by the computer's processor. Each processor has a different set of
instructions encoded by different machine code.

In this post, we will be generating machine code for the x86-64 CPU
architecture, which is the most common processor architecture used by personal
computers today. But you want to target mobile devices, or the newer Macs, you
would need to target an ARM architecture, like AArch64.

A JIT compiler is a compiler that compiles just-in-time. That is, it compiles
the program to machine code only just before executing it. This is the opposite
of what normal compilers do, like Rust, who compiles the entire program into
native executables before distributing them.

JIT compilers have a series of advantages, like not needing to distribute
multiple versions of each program compiled for each architecture and operating
system, or being able to apply optimizations specifics to the particular
computer and runtime workload. 

But also, it needs less knowledge to implement a JIT compiler than to create
a native executable, so it is easier to explore it here in this post (but
I plan to generate native executable in a later post).

# Basics of a JIT compiler

To write a JIT compiler, you need basically do things:

- Generate machine code.
- Execute that code, right after generating it.

### Generating machine code

Machine code is basically a binary encoding for the instructions that the
processor execute, and assembly is the human-readable encoding of these
instructions. And an assembler is the program that convert assembly to
the machine code.

For example, take the assembly instruction that was implementing the `+` in the
assembly at the start of the post, was `add byte ptr [r13 + rsi + 40], 1`,
which is an instruction that adds the immediate value `1` to the byte at the
address given by `r13 + rsi + 40`. 

If you follow the encoding steps for this instruction (which I tried, but
discovered that this would take much more time to learn that I expected), or
use an assembler as a sane person would do, you will discover that these
instructions is encoded by the byte sequence `0x41 0x80 0x44 0x35 0x28 0x01`.

And there is no secret for encoding multiples instructions, you only need
to encode each one, and concatenate them together.

And we also need to make sure to terminate our instructions sequence. In our
case it will be a `ret` instruction, which will return the execution from the
generated code, back to our Rust code, was we will see in the next section.
But this also could be a `exit` syscall, which would terminate the entire
program.

As an example, there is the assembly for a function that adds 1 to its 64-bit
argument and return it:

```nasm
add rdi, 1    ; the argument is in rdi, I add 1 to it.
mov rax, rdi  ; the return value is rax, so I move rdi to rax
ret           ; return from the function
```

Passing it through the GNU assembler, followed by objdump:

```shell
$ as -msyntax=intel -mnaked-reg add1.s -o add1.out
$ objdump -d add1.out

add1.out:     file format elf64-x86-64


Disassembly of section .text:

0000000000000000 <.text>:
   0:   48 83 c7 01             add    $0x1,%rdi
   4:   48 89 f8                mov    %rdi,%rax
   7:   c3                      retq
```

And we could write that in a rust program:

```rust
let add1 = [
    0x48, 0x83, 0xc7, 0x01, // add rdi, 1
    0x48, 0x89, 0xf8,       // mov rax, rdi
    0xc3,                   // ret
];
```

### Executing that code

And to execute that code we only need to cast it to a function pointer and call
it!

```rust
let add1: unsafe extern "sysv64" fn(u64) -> u64 = unsafe {
    std::mem::transmute(add1.as_ptr())
};

let x = unsafe {
    add1(1)
};

println!("{x}");
```

Well, no. If you run that it will only output "Segmentation fault".

But this is basically what we need to do. The problem is that operations
systems have restrictions on what memory regions have permission to be read,
write or execute. And by default, allocated memory has only read and write
permissions, they don't have execute permission to avoid [arbitrary code
execution] vulnerabilities.

[arbitrary code execution]: https://en.wikipedia.org/wiki/Arbitrary_code_execution

So to make this work, we only need to use some operating system API's that can
allocate memory with specified permissions, or can change the permission of
a memory region.

On POSIX systems, like Linux, that would be [`mmap`] and [`mprotect`]. On Windows,
that would be [`VirtualAlloc`] and [`VirtualProtect`].

[`mmap`]: https://man7.org/linux/man-pages/man2/mmap.2.html
[`mprotect`]: https://man7.org/linux/man-pages/man2/mprotect.2.html

[`VirtualAlloc`]: https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualalloc
[`VirtualProtect`]: https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualprotect

So in our Rust code, we could use the `libc` crate to call `mmap` on Linux, and
get an executable memory region:

```rust
let add1: extern "sysv64" fn(u64) -> u64 = unsafe {
    let mem = libc::mmap(
        std::ptr::null_mut(),
        add1.len(),
        libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    );
    
    if mem == libc::MAP_FAILED {
        println!("mmap failed");
        return;
    }

    let mem = std::slice::from_raw_parts_mut(mem as *mut u8, add1.len());

    mem.copy_from_slice(&add1);

    std::mem::transmute(mem.as_ptr())
};

let x = add1(1);

println!("{x}");
```

And if you run this ([here is a playground]), it prints `2`!

[here is a playground]: https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=51c45dc7ddcf6d8a21a3a306991e2e0e

But what we are doing is completely circumventing the memory protection.
Ideally we should never have exec and write permissions for the same memory.

We can achieve that by allocating memory with read and write permissions, write
the code to it, and finally change it to read and exec permissions. This can be
done with the help of `mprotect`, as long as the given pointer is page aligned,
which the `mmap` guaranties ([playground][mprot_playground]):

[mprot_playground]: https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=979e55c5cc620f1306415a29d134713a

```rust
let add1: extern "sysv64" fn(u64) -> u64 = unsafe {
    let mem = libc::mmap(
        std::ptr::null_mut(),
        add1.len(),
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    );

    if mem == libc::MAP_FAILED {
        println!("mmap failed");
        return;
    }

    std::slice::from_raw_parts_mut(mem as *mut u8, add1.len())
        .copy_from_slice(&add1);

    let result = libc::mprotect(
        mem,
        add1.len(),
        libc::PROT_READ | libc::PROT_EXEC,
    );

    if result == -1 {
        println!("mprotect failed");
        return;
    }

    std::mem::transmute(mem.as_ptr())
};
```

Also, notice that we are leaking the `mmap` allocated memory. You need to call
`munmap` to free it.

# The calling Convention

Before we start writing our JIT compiler we need to revise one last thing, the
calling convention. In the previous snippets I was casting the program code to
a `extern "sysv64" fn(u64) -> u64`, where the `extern "sysv64"` specifies that
that function pointer is using the System V ABI.

The ABI dictates, among other things, what is the [calling convention]. That is
what guarantees that the first argument will be in the `rdi` register, and the
return value should be in the `rax`, for example.

[calling convention]: https://wiki.osdev.org/System_V_ABI#x86-64

Another important point of the calling convention are the preserved registers,
and the scratch registers. The preserved registers are the ones that a function
call does not change. This means that when we generate our function, we need
to save these register on the stack before using them, and must restore them
before returning.

The scratch registers are the opposite, they can be modified by a function
call. This also means that if we need a value of one of these register, we
need to save them (in another register, or on the stack) before executing a
call.

So when we start writing our compiled code, we need to be sure not use a
scratch register to hold long persisted values (which will be only the address
of the memory and the value of the pointer), and save and restore any preserved
register that we might be using.

# Part 2 - A JIT Compiler

So now we have everything necessary to build our Brainfuck JIT compiler. We
will start with our basic interpreter, which will not have any of our applied
optimizations, except for the precomputed paring brackets, that will be
compiled down to conditional jumps.

First we need to decide which register will be used for what. I will use the
`r12` register for the address of the memory of the array of cells, which will
initially be passed as an argument in the `rdi`, and `r13` will hold the value
of the pointer.

So we can start our compiler by saving these two registers, and setting they
value. I copied the code from the optimized interpreter, keeping the `main`
function, but modifying the content of `Program` and its methods:

```rust
use std::io::Write;

struct Program {
    code: Vec<u8>,
    memory: [u8; 30_000],
}
impl Program {
    fn new(source: &[u8]) -> Result<Program, UnbalancedBrackets> {
        let mut code = Vec::new();

        // ; r12 will be the adress of `memory`
        // ; r13 will be the value of `pointer`
        // ; r12 is got from argument 1 in `rdi`
        // ; r13 is set to 0
        // push r12
        // push r13
        // mov r12, rdi
        // xor r13, r13
        code.write_all(&[
            0x41, 0x54, //
            0x41, 0x55, //
            0x49, 0x89, 0xfc, //
            0x4d, 0x31, 0xed,
        ])
        .unwrap();


        for b in source {
            match b {
                b'+' => todo!(),
                b'-' => todo!(),
                b'.' => todo!(),
                b',' => todo!(),
                b'<' => todo!(),
                b'>' => todo!(),
                b'[' => todo!(),
                b']' => todo!(),
                _ => continue,
            }
        }

        // ; when we push to the stack, we need to remeber
        // ; to pop them in the opossite order.
        // pop r13
        // pop r12
        // ret
        code.write_all(&[
            0x41, 0x5d, //
            0x41, 0x5c, //
            0xc3,
        ])
        .unwrap();

        Ok(Program {
            code,
            memory: [0; 30_000],
        })
    }

    // ...
}
```

Now we need to implement each instruction. For the `+` and `-` we can use the
already seen `add byte ptr [r13 + rsi + 40], 1`, but now using r12 and r13:


```rust
b'+' => {
    // add byte [r12 + r13], 1
    code.write_all(&[0x43, 0x80, 0x04, 0x2c, 0x01]).unwrap();
}
b'-' => {
    // add byte [r12 + r13], -1
    code.write_all(&[0x43, 0x80, 0x04, 0x2c, 0xff]).unwrap();
}
```

The `>` and `<` could be a single instruction, `add r13, 1` and `add r13, -1`,
but also need to implement the wrapping behavior. I used module operator to
implement that in the interpreter, but I discovered that operation is very slow
to perform, and the compiler decided to use some [compiler magic], that
resulted in that big series of multiplications and shifts in the disassembly
seen at the start of this post.

[compiler magic]: https://stackoverflow.com/questions/4361979/how-does-the-gcc-implementation-of-modulo-work-and-why-does-it-not-use-the

So instead, I will implement the wrapping using conditional logic. It is
uncertain if the branching will be more performant than the first
implementation, but it will be easier to write.

Using [this playground's ASM output], I got the following implementation:

[this playground's ASM ouput]: https://play.rust-lang.org/?version=stable&mode=release&edition=2021&gist=ab83982a30b0712f0985a99d1658717f

```nasm
playground::add1:
	lea	rcx, [rdi + 1]
	xor	eax, eax
	cmp	rcx, 30000
	cmovne	rax, rcx
	ret

playground::sub1:
	sub	rdi, 1
	mov	eax, 29999
	cmovae	rax, rdi
	ret
```

Which does not contain branching at all! Only a conditional move (didn't know
that instructions existed). So there is a high change that this will be better
than the previous optimization.

So, after changing the register to the ones we are using, and inverting some
conditions, we get the following:

```rust
b'<' => {
    // sub r13, 1
    // mov eax, 29999
    // cmovb r13, rax
    code.write_all(&[
        0x49, 0x83, 0xed, 0x01, //
        0xb8, 0x2f, 0x75, 0x00, 0x00, //
        0x4c, 0x0f, 0x42, 0xe8,
    ])
    .unwrap();
}
b'>' => {
    // add r13, 1
    // xor eax, eax
    // cmp r13, 30000
    // cmove r13, rax
    code.write_all(&[
        0x49, 0x83, 0xc5, 0x01, //
        0x31, 0xc0, //
        0x49, 0x81, 0xfd, 0x30, 0x75, 0x00, 0x00, //
        0x4c, 0x0f, 0x44, 0xe8,
    ])
    .unwrap();
}
```

Now lets address the `.` and `,`. These operators interact with the stdout and
stdin. To be able to access them, we will need to make use of system calls.

A system call, or syscall, is an instruction that call a function that lives in
the operating system kernel. We will need to use the [`read`] and [`write`]
syscalls.

[`read`]: https://man7.org/linux/man-pages/man2/read.2.html
[`write`]: https://man7.org/linux/man-pages/man2/write.2.html

These syscall are the ones that read and write from a file descriptor. To call
them you need to set the syscall call number to `rax`, pass its arguments using
the calling convention, and finally execute the instruction `syscall`.

The `write` syscall, on x86, has call number 1, and receives as parameter the
file descriptor (which for stdout is 1) and a pointer and a length for the
buffer whose content will be written. It returns the number of bytes written, or
-1 in case of error.

You are expected to handle the error, or cases where the entire buffer was not
written, but I will avoid writing too much complicate assembly for now, it will
already be enough for our case:

```rust
b'.' => {
    // mov rax, 1 ; write syscall
    // mov rdi, 1 ; stdout's file descriptor
    // lea rsi, [r12 + r13] ; buf address
    // mov rdx, 1           ; length
    // syscall
    code.write_all(&[
        0xb8, 0x01, 0x00, 0x00, 0x00, //
        0xbf, 0x01, 0x00, 0x00, 0x00, //
        0x4b, 0x8d, 0x34, 0x2c, //
        0xba, 0x01, 0x00, 0x00, 0x00, //
        0x0f, 0x05, //
    ])
    .unwrap();
}
```

The `read` syscall is very similar, but its call number is 0, but it writes to
the passed buffer. Also, now we pass the file descriptor 0, which is the stdin:

```rust
b',' => {
    // mov rax, 0 ; read syscall
    // mov rdi, 0 ; stdin's file descriptor
    // lea rsi, [r12 + r13] ; buf address
    // mov rdx, 1           ; length
    // syscall
    code.write_all(&[
        0xb8, 0x00, 0x00, 0x00, 0x00, //
        0xbf, 0x00, 0x00, 0x00, 0x00, //
        0x4b, 0x8d, 0x34, 0x2c, //
        0xba, 0x01, 0x00, 0x00, 0x00, //
        0x0f, 0x05, //
    ])
    .unwrap();
}
```

And finally we need to implement `[` and `]`, which is the tricker. To
implement it we will need to write a conditional relative jump. In assembly, it
will be like:

```nasm
cmp byte [r12+r13], 0
je near <jump_offset>
```

That is, we compare the value of the current cell to 0, and will jump if it is
equal (`je`) or if not equal (`jne`) depending on whether it is `[` or `]`. The jump
instruction receives as parameter the offset of the jump, and the `near`
keyword there is to tell that the offset will be a `i32` value. Could be a
smaller value (by using `short`), depending on how big the jump is, but
checking the jump length would complicate things a little, so let's keep it
simple.

First, to write the machine code of that instruction we need to know how the
offset is encoded in it. Thankfully the instruction encoding is very simple, it
is only two bytes of the opcode followed by the four little endian bytes of the
offset.

But we also need to discover the offset. For that, we will do something very
similar to the pair address precomputation in the previous post.

First we create a stack. And every time we encounter a `[`, we write its
implementation (with the jump filled with a dummy offset) and push the byte
index of the next instruction to the stack.

```rust
let mut bracket_stack = Vec::new();

for b in source {
    // ...
```

```rust
b'[' => {
    // ; note that the offset of 0 is a dummy value,
    // ; it will be fixed in the pair `]`
    // cmp byte [r12+r13], 0
    // je near .END
    code.write_all(&[
        0x43, 0x80, 0x3c, 0x2c, 0x00, //
        0x0f, 0x84, 0x00, 0x00, 0x00, 0x00,
    ])
    .unwrap();

    // push to the stack the byte index of the next instruction.
    bracket_stack.push(code.len() as u32);
}
```

And every time we encounter a `]`, we pop the index from the stack, compute the
offset, and fix the offset of the last jump. Note that the offset that the
encoded jump receives is from the address of the instruction after the jump, to
the address of the target instruction.

```rust
b']' => {
    let left = match bracket_stack.pop() {
        Some(x) => x as usize,
        None => return Err(UnbalancedBrackets(']', code.len())),
    };

    // cmp byte [r12 + r13], 0
    // jne near .START
    code.write_all(&[
        0x43, 0x80, 0x3c, 0x2c, 0x00, //
        0x0f, 0x85, 0xf1, 0xff, 0xff, 0xff,
    ])
    .unwrap();

    // the byte index of the next instruction
    let right = code.len();

    let offset = right as i32 - left as i32;

    // fix relative jumps offsets
    code[left - 4..left].copy_from_slice(&offset.to_le_bytes());
    code[right - 4..right].copy_from_slice(&(-offset).to_le_bytes());
}
```

And that is it! Now we only need to write the generated code to an executable
buffer, cast to pointer and call it:

```rust
fn run(&mut self) -> std::io::Result<()> {
    unsafe {
        let len = self.code.len();
        let mem = libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );

        if mem == libc::MAP_FAILED {
            panic!("mmap failed");
        }

        // SAFETY: mem is zero initalized by the mmap.
        std::slice::from_raw_parts_mut(mem as *mut u8, len).copy_from_slice(&self.code);

        // mem.as_ptr() is page aligned, because it is get from mmap.
        let result = libc::mprotect(mem, len, libc::PROT_READ | libc::PROT_EXEC);

        if result == -1 {
            panic!("mprotect failed");
        }

        let code_fn: unsafe extern "sysv64" fn(*mut u8) = std::mem::transmute(mem);

        code_fn(self.memory.as_mut_ptr());

        let result = libc::munmap(mem, len);

        if result == -1 {
            panic!("munmap failed");
        }
    }
    Ok(())
}
```

And after using a debugger to step through the generated assembly and fixing
errors in the assembly, I finally get to measure its performance:

![](/assets/brainfuck/plot8.svg){:style="display:block; margin-left:auto; margin-right:auto"}

It is almost 7 times faster than the interpreted one! It is within the
expected performance increase from the analysis of the interpreter overhead. 

The comparison is not yet fair with the basic interpreter, because we still
need to check and return the errors from the `write` and `read` syscall and
implement the EOF behavior. But these changes will only make it a little
slower (only a little, IO is the least common instruction).

Even so, the optimized interpreter is still faster than this JIT compiler.
Nevertheless, nothing prevent us to apply the exact optimizations used in the
interpreter, and on top of that compile them to machine code.

But before we continue further, we first need to address how we can
improve our workflow.

# Improving our workflow with Dynasm

My workflow for writing this JIT compiler was the following:

1. Inspect some assembly emitted from Rust, to learn how to implement the
   instruction.
2. Write its assembly to a temporary file.
3. Assemble and disassemble the assembly to get the machine code, using `nasm
   test.s -felf64 && objdump -M intel -d test.o`
4. Copy the hexadecimal output to my JIT source code.
5. Test it, go back to step 2 if failed.

This workflow is not so bad, at least I don't need to read Intel's
documentation to discover how to encode the instructions. Well... I still need
to search a little when I need to dynamically set some parameter of the
instruction.

Steps 1 and 5 are unavoidable, but steps 2, 3 and 4 creates a big overhead over
the iterative process, and introduce new error points. And if somehow a typo is
introduced in one the bytes of the machine code, the only reasonable way of
discovery which one is to disassemble them again.

What would really help would be if I could write the assembly directly in my
Rust source code, and it assembles it to a byte sequence at compile time. Better
yet if it could automatically figure out how to inject dynamic parameter in the
machine code.

And every thing describe here can be implemented using Rust procedural macros.
And has already been done.

[Dynasm-rs] is a dynamic assembler for Rust, a tool that ease the creation of
programs that require run-time assembling. It is inspired by LuaJIT's famous
[DynASM], but uses procedural macros instead of a C preprocessor.

[dynasm-rs]: https://github.com/CensoredUsername/dynasm-rs
[DynASM]: https://luajit.org/dynasm.html

To us, this means that we replace code like this:

```rust
// push r12
// push r13
// mov r12, rdi
// xor r13, r13
code.write_all(&[
    0x41, 0x54, //
    0x41, 0x55, //
    0x49, 0x89, 0xfc, //
    0x4d, 0x31, 0xed,
])
.unwrap();
```

By this:

```rust
dynasm!(code
    ; .arch x64
    ; push r12
    ; push r13
    ; mov r12, rax
    ; xor r13, r13
)
```

Much easier to maintain. And the `dynasm!` macro compiles that down to a single
`code.extend()` call. But for the remainder of the assembly blocks, dynasm will
rely on more methods of its [`DynasmApi`], that can be found in the `dynasmrt` crate.

[`DynasmApi`]: https://docs.rs/dynasmrt/1.2.3/dynasmrt/trait.DynasmApi.html

But we will need only a handful of them, so we don't need to import
`dynasmrt` (yet), and instead copy the trait to our code:

```rust
trait DynasmApi {
    fn push_i8(&mut self, value: i8);
    fn push_i32(&mut self, value: i32);
}
impl DynasmApi for Vec<u8> {
    fn push_i8(&mut self, value: i8) {
        self.push(value as u8)
    }
    fn push_i32(&mut self, value: i32) {
        self.extend_from_slice(&value.to_le_bytes())
    }
}
```

That will be enough to replace all raw machine code blocks:

```rust
fn new(source: &[u8]) -> Result<Program, UnbalancedBrackets> {
    let mut code = Vec::new();

    // r12 will be the adress of `memory`
    // r13 will be the value of `pointer`
    // r12 is got from argument 1 in `rdi`
    // r13 is set to 0
    dynasm! { code
        ; .arch x64
        ; push r12
        ; push r13
        ; mov r12, rdi
        ; xor r13, r13
    };

    let mut bracket_stack = Vec::new();

    for b in source {
        match b {
            b'+' => dynasm! { code
                ; .arch x64
                ; add BYTE [r12 + r13], 1
            },
            b'-' => dynasm! { code
                ; .arch x64
                ; add BYTE [r12 + r13], -1
            },
            b'.' => dynasm! { code
                ; .arch x64
                ; mov rax, 1 // write syscall
                ; mov rdi, 1 // stdout's file descriptor
                ; lea rsi, [r12 + r13] // buf address
                ; mov rdx, 1           // length
                ; syscall
            },
            b',' => dynasm! { code
                ; .arch x64
                ; mov rax, 0 // read syscall
                ; mov rdi, 0 // stdin's file descriptor
                ; lea rsi, [r12 + r13] // buf address
                ; mov rdx, 1           // length
                ; syscall
            },
            b'<' => dynasm! { code
                ; .arch x64
                ; sub r13, 1
                ; mov eax, 29999
                ; cmovb r13, rax
            },
            b'>' => dynasm! { code
                ; .arch x64
                ; add r13, 1
                ; xor eax, eax
                ; cmp r13, 30000
                ; cmove r13, rax
            },
            b'[' => {
                // note that the offset of 0 is a dummy value,
                // it will be fixed in the pair `]`
                dynasm! { code
                    ; .arch x64
                    ; cmp BYTE [r12+r13], 0
                    ; je i32::max_value()
                };

                // push to the stack the byte index of the next instruction.
                bracket_stack.push(code.len() as u32);
            }
            b']' => {
                let left = match bracket_stack.pop() {
                    Some(x) => x as usize,
                    None => return Err(UnbalancedBrackets(']', code.len())),
                };

                dynasm! { code
                    ; .arch x64
                    ; cmp BYTE [r12 + r13], 0
                    ; jne i32::max_value()
                };

                // the byte index of the next instruction
                let right = code.len();

                let offset = right as i32 - left as i32;

                // fix relative jumps offsets
                code[left - 4..left].copy_from_slice(&offset.to_le_bytes());
                code[right - 4..right].copy_from_slice(&(-offset).to_le_bytes());
            }
            _ => continue,
        }
    }

    if !bracket_stack.is_empty() {
        return Err(UnbalancedBrackets(']', code.len()));
    }

    // when we push to the stack, we need to remember
    // to pop them in the opposite order.
    dynasm! { code
        ; .arch x64
        ; pop r13
        ; pop r12
        ; ret
    }

    Ok(Program {
        code,
        memory: [0; 30_000],
    })
}
```

Much better! But we still are relying on in how the offset of the jump is encoded
int the machine code. But thankfully dynasm can also handle that, we only need
to start using the `dynasmrt` crate.

So first can replace our simple `Vec<u8>` + ad-hoc extension trait, by
`dynasmrt::VecAssembler`:

```rust
use dynasmrt::{dynasm, x64::X64Relocation, DynasmApi, DynasmLabelApi, VecAssembler};

let code = VecAssembler<X64Relocation>::new();
```

Now, on `[`, we create two dynamic labels, one for the start of the loop, and
one for the end. We use `=>` to use the dynamic label in the assembly, the
`end_label` go as the parameter of the jump, and `start_label` goes to a
standalone line, defining the label position. And we finally we put both labels
at the stack.

```rust
b'[' => {
    let start_label = code.new_dynamic_label();
    let end_label = code.new_dynamic_label();
    dynasm! { code
        ; .arch x64
        ; cmp BYTE [r12+r13], 0
        ; je =>end_label
        ; =>start_label
    };

    bracket_stack.push((start_label, end_label));
}
```

And on `]`, we pop the labels from the stack, and use them in
a similar way, but with opposite labels:

```rust
b']' => {
    let (start_label, end_label) = match bracket_stack.pop() {
        Some(x) => x,
        None => return Err(UnbalancedBrackets(']', code.offset().0)),
    };

    dynasm! { code
        ; .arch x64
        ; cmp BYTE [r12 + r13], 0
        ; jne =>start_label
        ; => end_label
    };
}
```

And with that we used all dynasm features that we need for writing the assembly.
But it also has some other help utilities for running our generated code.

We can replace our unsafe calls to `mmap`, `mprotect` and `munmap` by dynasm's
`MutableBuffer` and `ExecutableBuffer`. Not only they encapsulate the unsafe 
code for us, but also provides a nice cross-platform abstraction[^memmap2],
that will be useful for my eventual Windows support.

[^memmap2]: Actually these dynasm types uses [memmap2] abstractions underneath,
  the de facto crate for memory mapping.

[memmap2]: https://crates.io/crates/memmap2

So, our `Program::run` becomes:

```rust
fn run(&mut self) -> std::io::Result<()> {
    let mut buffer = MutableBuffer::new(self.code.len()).unwrap();
    buffer.set_len(self.code.len());

    buffer.copy_from_slice(&self.code);

    let buffer = buffer.make_exec().unwrap();

    unsafe {
        let code_fn: unsafe extern "sysv64" fn(*mut u8) = std::mem::transmute(buffer.as_ptr());
        code_fn(self.memory.as_mut_ptr());
    }

    Ok(())
}
```

# Implement IO error handling and EOF behavior

Now with our enhanced workflow, we can continue to implement the final details
necessary for our JIT compiler be 100% compatible with our basic interpreter.
For that we need to retry the read/write syscall until all bytes are written,
check if any of the calls return an error and return to the runtime, and check
if reach the end-of-file of the stdin and fallback to the value 0.

That is a lot of code to be implemented in assembly. But thankfully we not need
to! We can instead implement every thing in Rust (minus the return of the
error)!

We only need to declare a Rust function with the proper ABI, and call it
directly from the assembly.

Not only it will make implementing all this much easier, we also get a
cross-platform implementation for free.

We can directly copy the code from our interpreter and put in a `extern
"sysv64" fn(*const u8)` function, and call it from our generated code.

But we need to adapt it to also return the IO error. Because Rust ABI is not
stable, we cannot return the `Error` type directly. So instead I will put
it in a Box, and return its pointer, using `Box::leak()`.

Putting everything together, the read and write functions become:

```rust
extern "sysv64" fn write(value: u8) -> *mut std::io::Error {
    // Writing a non-UTF-8 byte sequence on Windows error out.
    if cfg!(target_os = "windows") && value >= 128 {
        return std::ptr::null_mut();
    }

    let mut stdout = std::io::stdout().lock();

    let result = stdout.write_all(&[value]).and_then(|_| stdout.flush());

    match result {
        Err(err) => Box::leak(Box::new(err)) as *mut _,
        _ => std::ptr::null_mut(),
    }
}

unsafe extern "sysv64" fn read(buf: *mut u8) -> *mut std::io::Error {
    let mut stdin = std::io::stdin().lock();
    loop {
        let mut value = 0;
        let err = stdin.read_exact(std::slice::from_mut(&mut value));

        if let Err(err) = err {
            if err.kind() != std::io::ErrorKind::UnexpectedEof {
                return Box::leak(Box::new(err));
            }
            value = 0;
        }

        // ignore CR from Window's CRLF
        if cfg!(target_os = "windows") && value == b'\r' {
            continue;
        }

        *buf = value;

        return std::ptr::null_mut();
    }
}
```

Now we only need to call these functions in the assembly. For that we get the 
functions addresses, move them to a register, and do indirect call from the register.
The call will return a non-null pointer in case of an error, so we can check that
and return early if necessary.

To return early we will a jump to a dynasm's global label:

```rust
b'.' => {
    dynasm! { code
        ; .arch x64
        ; mov rax, QWORD write as *const () as i64
        ; mov rdi, [r12 + r13] // cell value
        ; call rax
        ; cmp rax, 0
        ; jne ->exit
    }
}
b',' => {
    dynasm! { code
        ; .arch x64
        ; mov rax, QWORD read as *const () as i64
        ; lea rdi, [r12 + r13] // cell address
        ; call rax
        ; cmp rax, 0
        ; jne ->exit
        ; ret
    }
}
```

And we define that label at the end, before we pop the stack and return:

```rust
// when we push to the stack, we need to remeber
// to pop them in the opossite order.
dynasm! { code
    ; .arch x64
    ; xor rax, rax // clear `rax` in case of no early return
    ; ->exit:
    ; pop r13
    ; pop r12
    ; ret
}
```

Finally, we get the error back to Rust, after calling the generated code:

```rust
unsafe {
    let code_fn: unsafe extern "sysv64" fn(*mut u8) -> *mut std::io::Error =
        std::mem::transmute(buffer.as_ptr());

    let error = code_fn(self.memory.as_mut_ptr());

    if !error.is_null() {
        return Err(*Box::from_raw(error));
    }
}
```

And done! We have an implementation that is 100% comparable with our first
interpreter. Besides, these changes was already enough make the program
compatible with Windows.

If we look at its performance:

![](/assets/brainfuck/plot9.svg){:style="display:block; margin-left:auto; margin-right:auto"}

It continues to be within the margin of error of the previous one.

# An Optimized JIT compiler

And now implementing A JIT compiled version of our optimized interpreter becomes
very straightforward. For that I will just copy the `enum Instruction` and
parsing code from the interpreter, and generate the machine from that. 

This implementation will no longer be strictly speaking a single-pass
implementation, because we are collection all instructions before doing the
compilation. For this to remain as a single-pass implementation, we would need
to emit and process `Instructions` one at a time. This can be done, but because
some instructions are made up from smaller instruction (like `Clear`), this
would be a little trick. But let's not focus on that.

[Here is the commit where I added the enum and parsing]. I also clean up the
unnecessary precomputation of paring bracket, that now happens in compilation.

Now we can start writing the compilation, of the instructions. In this case,
the implementation of `JumpRight`, `JumpLeft`, `Input` and `Ouput` continued to
be exactly the same.

For the `Add(u8)`, we only need to use `value` as the operant of the
instructions, instead of the hard-coded `1`:

```rust
Instruction::Add(n) => dynasm! { code
    ; .arch x64
    ; add BYTE [r12 + r13], BYTE n as i8
},
```

When implementing the `Move(usize)` instruction, I notice that x86 does not have
add immediate for 64-bit values, so I downgrade all `usize`s used in
instructions to `i32`.

I use [this other playground] to get the assembly for the `Move(i32)`:

[this other playground]: https://play.rust-lang.org/?version=nightly&mode=release&edition=2021&gist=c06d28e6fc3f8d20da3fdaae233af22f

```rust
Instruction::Move(n) => {
    if n > 0 {
        dynasm! { code
            ; lea eax, [r13 + n]
            ; add r13, -(30000 - n)
            ; cmp eax, 30000
            ; cmovl r13d, eax
        }
    } else {
        dynasm! { code
            ; lea eax, [r13 + n]
            ; add r13d, 30000 + n
            ; test eax, eax
            ; cmovns r13d, eax
        }
    }
}
```

This show one of the benefits of JIT compilation, I only know if `n` is positive
or negative at runtime, but because I JIT compiling, I generate code optimized
for each case.


For the `Clear` we simply move 0 to the cell.

```rust
Instruction::Clear => dynasm! { code
    ; .arch x64
    ; mov BYTE [r12 + r13], 0
},
```

For the `AddTo(i32)`, we compute that index of the target cell similarly to the
`Move(i32)` instruction, but with the result in `rax`, and finish by reading the
current cell, clearing it, and adding to the target cell.

```rust
Instruction::AddTo(n) => dynasm! { code
    ; .arch x64
    // rax = cell to add to
    ;;
    if n > 0 {
        dynasm! { code
            ; lea ecx, [r13 + n]
            ; lea eax, [r13 + n - 30000]
            ; cmp ecx, 30000
            ; cmovl eax, ecx
        }
    } else {
        dynasm! { code
            ; lea ecx, [r13 + n]
            ; lea eax, [r13 + 30000 + n]
            ; test ecx, ecx
            ; cmovns eax, ecx
        }
    }
    ; mov cl, [r12 + r13]
    ; add BYTE [r12 + rax], cl
    ; mov BYTE [r12 + r13], 0
},
```

And for the `MoveUntil(i32)` instructions we, create a loop where we check for
zero, exit the loop if true, otherwise do a move and repeat.

Here I used two unseen features of dynasm. Local labels, denoted by `:` and `<`
or `>` depending on if the jump is backwards or forwards, and `;;` that allow us
to include Rust code inside the macro. Also notice that I copied the code from
`Move(i32)` here.

```rust
Instruction::MoveUntil(n) => dynasm! { code
    ; .arch x64

    ; repeat:

    // check if 0
    ; cmp BYTE [r12 + r13], 0
    ; je >exit

    // Move n
    ;;
    if n > 0 {
        dynasm! { code
            ; lea eax, [r13 + n]
            ; add r13, -(30000 - n)
            ; cmp eax, 30000
            ; cmovl r13d, eax
        }
    } else {
        dynasm! { code
            ; lea eax, [r13 + n]
            ; add r13d, 30000 + n
            ; test eax, eax
            ; cmovns r13d, eax
        }
    }

    ; jmp <repeat

    ; exit:
},
```

And our optimized JIT compiler is complete! Measuring its performance:

![](/assets/brainfuck/plot10.svg){:style="display:block; margin-left:auto; margin-right:auto"}

And was expected, the performance increase is big! More than 3x faster.


# Future work

![](/assets/brainfuck/plot11.svg){:style="display:block; margin-left:auto; margin-right:auto"}

We manage to create more than 26x performance increase since our first
Interpreter. But we can always do better. One possibility is to investigate what
choice of instructions could result in a better performance. For example, using
`xor eax, eax` instead of `xor rax, rax` would result in a smaller instruction,
and using `mov [r12 + r13], r14` where `r14` is register with the value 0 would
be faster than `mov [r12 + r13], 0`. We could also check if two instructions are
reading the same cell twice, and optimize that. Etc.

But these types of optimizations need a deep understanding of the target
instructions set and how each instruction is executed by the processor
(knowledge that I don't have). And this knowledge need to exist for each
architecture that your program is targeting, and there are dozens of them!

To solve that problem is why exist projects such as [GCC] and [LLVM]. They make
that languages implementations just need to worry about emitting they code to
these projects corresponding intermediate representation (IR), and they take the
job of choosing the optimal instructions to emit. And they also take care of
applying optimizations to the code.

[gcc]: https://gcc.gnu.org/
[LLVM]: https://llvm.org/

In fact, the optimizations applied by these projects are so good that they
probably could automatically apply all the optimizations that we have come up.

And this is exactly what we will explore in the next post in this series. The
original series by Eli Bendersky have [made use of LLVM], but I follow a more
Rusty approach and use [Cranelift] instead. Cranelift is a code generator
written in Rust, and used by projects such as [Wasmtime] and
[rustc_codegen_cranelift]. It does not apply a lot of optimizations, so maybe we
don't get it to our optimized JIT implementation (we will see). But it does
solve the problem of multiple architecture targeting, so I will be able to run
our compiler on my Android device, that uses ARM.

[made use of LLVM]: https://eli.thegreenplace.net/2017/adventures-in-jit-compilation-part-3-llvm/
[Cranelift]: https://github.com/bytecodealliance/wasmtime/tree/main/cranelift
