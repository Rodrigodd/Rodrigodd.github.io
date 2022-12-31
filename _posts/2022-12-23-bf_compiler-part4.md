---
layout: post
title:  "Compiling Brainfuck code - Part 4: A Static Compiler"
date:   2022-11-26 18:00:00 -0300
---

This is the third post of a blog post series where I reproduce [Eli Bendersky’s
Adventures In JIT Compilation series][eli], but this time using the [Rust
programming language][rust].

[eli]: https://eli.thegreenplace.net/2017/adventures-in-jit-compilation-part-1-an-interpreter
[Rust]: https://www.rust-lang.org

- [Part 1: An Optimized Interpreter]({% post_url 2022-10-16-bf_compiler-part1 %})
- [Part 2: A Singlepass JIT Compiler]({% post_url 2022-10-16-bf_compiler-part2 %})
- [Part 3: A Cranelift JIT Compiler]({% post_url 2022-11-21-bf_compiler-part3 %})
- **Part 4: A Static Compiler**

In this final part we will use the compiled code that we generated in the
previous parts, and learn how to put them in an executable file, achieving a
static compiler.

# Executable files and Object Files

To build a static compiler, we need to be able to save our generated assembly in
a file, in a way that the operating system can execute.

Each OS have its own executable file format. On Windows, executable files are in
the [PE] file format, on macOS in the [Mach-O]. On Linux, executable files are in
the [ELF] file format, which we will be exploring a little more in deep here.

[PE]: https://learn.microsoft.com/en-us/windows/win32/debug/pe-format
[Mach-O]: https://github.com/apple-oss-distributions/xnu/blob/xnu-8792.61.2/EXTERNAL_HEADERS/mach-o/loader.h
[ELF]: https://man7.org/linux/man-pages/man5/elf.5.html

Another important piece of the compilation process are the object files, and the
process of linking. Object files contain compiled code, but only part of the
code that forms a program. They are used by compiler to split the source code
compilation (C compilers can compile each `.c` file to a separated object file),
or for distributing precompiled libraries, for example. Again, each OS have its
own object file format, which commonly uses the same format as executables, or
executables use an extended version of them. On Linux, objects are also ELF files.

To link all these object files into a complete and executable program, you use a
linker. Each object file has a list of symbols, that, among other things, are
used to indicate what function the file define, and which ones they are using
(that may be defined in another file). It also has a list of relocations, that
are used to indicated which points in the memory need to be modified, and how,
to connect the object files. The linker is the program that read these files and
does all these modifications, and outputs an executable.

We will see more practical examples of this further down.

# Exploring ELF files

As always, we will start by making a compiler for the simplest program that we
can achieve. In the previous parts we were using the `add1` function as example,
but that don't make into an interesting enough program. So let's make a good old
"hello world" program.

On Linux x86-64 assembly, it would be:

```nasm
section .data
    hello:    db 'Hello world!',10
    helloLen: equ $-hello

section .text
    global _start 

_start:
    mov rax, 1           ; 'write' system call = 4
    mov rdi, 1           ; file descriptor 1 = STDOUT
    mov rsi, hello       ; string to write
    mov rdx, helloLen    ; length of string to write
    syscall              ; call the kernel

    ; Terminate program
    mov rax, 60          ; 'exit' system call = 60
    mov rdi, 0           ; exit with error code 0
    syscall              ; call the kernel
```

In the code above we have two sections. Each section holds a chunk arbitrary
data. The sections can have any name, but the linker will map the section with
the name `.data` to a segment that will have a read and write permissions when
executed, and the `.text` to a segment with read and execute permissions.

In the section `.data` we declared the string `'Hello World!\n'` (the 10 there
is the ASCII code for line-break) with the label `hello`, and a constant for the
string length (compute by taking the current address (`$`) minus the `hello`
address).

And in the section `.text` we declare that the label `_start` is global (meaning
that other objects can see it), and declaring it preceding the code for printing
the string (using the 'write' syscall) and exiting (with the 'exit' syscall).

Every label above will map to a symbol in the object file, that later will be
used for applying relocations, debug purposes, etcetera. The `_start` symbol in
specific will be used by the linker to tell where is the entry point of our
executable (and that is why it must be global).

To assemble and link the program, we can use the assembler [NASM] and the linker
[ld]:

[NASM]: https://www.nasm.us/
[ld]: https://man7.org/linux/man-pages/man1/ld.1.html

```shell
$ nasm hello.as -f elf64 -o hello.o
$ ld hello.o -o hello
```

And running it:

```shell
$ ./hello
Hello World!
```

It works!

Now, let's take a close look in the object file and the executable file. For
this, we can use the [readelf] utility. You can use the `-a` flag to show in one
go all the information that I will be explaining, but I will go in parts here.

[readelf]: https://man7.org/linux/man-pages/man1/readelf.1.html

First, lake take a look at the `hello.o` object file:

```
$ readelf hello.o -h
ELF Header:
  Magic:   7f 45 4c 46 02 01 01 00 00 00 00 00 00 00 00 00
  Class:                             ELF64
  Data:                              2's complement, little endian
  Version:                           1 (current)
  OS/ABI:                            UNIX - System V
  ABI Version:                       0
  Type:                              REL (Relocatable file)
  Machine:                           Advanced Micro Devices X86-64
  Version:                           0x1
  Entry point address:               0x0
  Start of program headers:          0 (bytes into file)
  Start of section headers:          64 (bytes into file)
  Flags:                             0x0
  Size of this header:               64 (bytes)
  Size of program headers:           0 (bytes)
  Number of program headers:         0
  Size of section headers:           64 (bytes)
  Number of section headers:         7
  Section header string table index: 3
```

First we have the ELF header, with basic information like version, ABI, etc. It
also contains the location in the file of the program headers (only relevant for
executable files) and the section headers.

You may want to take a look on [the man page for the ELF format][ELF] to see in
more detail what each field in the header means, and also for the parts below.

```shell
$ readelf hello.o -S
There are 7 section headers, starting at offset 0x40:

Section Headers:
  [Nr] Name              Type             Address           Offset
       Size              EntSize          Flags  Link  Info  Align
  [ 0]                   NULL             0000000000000000  00000000
       0000000000000000  0000000000000000           0     0     0
  [ 1] .data             PROGBITS         0000000000000000  00000200
       000000000000000d  0000000000000000  WA       0     0     4
  [ 2] .text             PROGBITS         0000000000000000  00000210
       0000000000000027  0000000000000000  AX       0     0     16
  [ 3] .shstrtab         STRTAB           0000000000000000  00000240
       0000000000000032  0000000000000000           0     0     1
  [ 4] .symtab           SYMTAB           0000000000000000  00000280
       00000000000000a8  0000000000000018           5     6     8
  [ 5] .strtab           STRTAB           0000000000000000  00000330
       0000000000000020  0000000000000000           0     0     1
  [ 6] .rela.text        RELA             0000000000000000  00000350
       0000000000000018  0000000000000018           4     2     8
Key to Flags:
  W (write), A (alloc), X (execute), M (merge), S (strings), I (info),
  L (link order), O (extra OS processing required), G (group), T (TLS),
  C (compressed), x (unknown), o (OS specific), E (exclude),
  l (large), p (processor specific)
```

Following it, we have the section headers. The first header is always a null
one. Here we can see that we have the headers for the `.data` and `.text`
sections that we have written in our assembly code. But we also have some other
sections that hold, respectively, the strings for the name of the sections
headers, the symbols, the string for the symbol names and such, and lastly the
relocations for the .text section.

Each section header tells some information about the section, like where its
content is in the file. It may also be linked to another section (like `.symtab`
has a link to the table that contains the symbol names).

The readelf can pretty print the contents of some sections, as we will see in a
moment, but you can also use `-x` to view they hex dump:

```shell
$ readelf hello.o -x .shstrtab

Hex dump of section '.shstrtab':
  0x00000000 002e6461 7461002e 74657874 002e7368 ..data..text..sh
  0x00000010 73747274 6162002e 73796d74 6162002e strtab..symtab..
  0x00000020 73747274 6162002e 72656c61 2e746578 strtab..rela.tex
  0x00000030 7400                                t.
```

Here for example we can see that the section "section header string table" is a
list of null byte terminated strings, with the first string being empty.

```shell
$ readelf hello.o -s

Symbol table '.symtab' contains 7 entries:
   Num:    Value          Size Type    Bind   Vis      Ndx Name
     0: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT  UND
     1: 0000000000000000     0 FILE    LOCAL  DEFAULT  ABS hello.as
     2: 0000000000000000     0 SECTION LOCAL  DEFAULT    1
     3: 0000000000000000     0 SECTION LOCAL  DEFAULT    2
     4: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT    1 hello
     5: 000000000000000d     0 NOTYPE  LOCAL  DEFAULT  ABS helloLen
     6: 0000000000000000     0 NOTYPE  GLOBAL DEFAULT    2 _start
```

With `-s` we can see the symbol table. The first one is always null. There are
the symbols that we defined in our assembly, and some others introduced by the
assembler like the file name and symbols for each section (used for relocation).

Each symbol contains a value. In the case of `hello` and `start` it is the
address of the symbol in its respective section (both are at the start of the
section, so they are 0) and in `helloLen` is the length of the string.

```shell
$ readelf hello.o -r

Relocation section '.rela.text' at offset 0x350 contains 1 entry:
  Offset          Info           Type           Sym. Value    Sym. Name + Addend
00000000000c  000200000001 R_X86_64_64       0000000000000000 .data + 0
```

And lastly we have the relocations. Here we have only a single one for the
`.text` section that applies at offset `0xc`, use the symbol 2 (the first 2
bytes of info) on the linked section (`.symtab`), is of type `R_X86_64_64` (each
processor has its own set of relocation types) and has an addend of 0. This
means that it replaces a 64bit word at `0xc` by the value of the symbol `.data`.

If we look at the disassembly of the machine code, we can see that in offset
`0xc` we have the last 8 bytes of the `movabs` instruction, the one that loads
the address of the `hello` string:

```shell
$ objdump -d -M intel hello.o

hello.o:     file format elf64-x86-64


Disassembly of section .text:

0000000000000000 <_start>:
   0:   b8 01 00 00 00          mov    eax,0x1
   5:   bf 01 00 00 00          mov    edi,0x1
   a:   48 be 00 00 00 00 00    movabs rsi,0x0
  11:   00 00 00
  14:   ba 0d 00 00 00          mov    edx,0xd
  19:   0f 05                   syscall
  1b:   b8 3c 00 00 00          mov    eax,0x3c
  20:   bf 00 00 00 00          mov    edi,0x0
  25:   0f 05                   syscall
```

This means that during linking, after the linker have set the location in memory
that the `data` section should be loaded, it will path that instruction to the
correct address of the `hello` string.

And we can see all this in action, by inspecting the `./hello` executable file:

```shell
$ readelf hello -S
There are 6 section headers, starting at offset 0x2158:

Section Headers:
  [Nr] Name              Type             Address           Offset
       Size              EntSize          Flags  Link  Info  Align
  [ 0]                   NULL             0000000000000000  00000000
       0000000000000000  0000000000000000           0     0     0
  [ 1] .text             PROGBITS         0000000000401000  00001000
       0000000000000027  0000000000000000  AX       0     0     16
  [ 2] .data             PROGBITS         0000000000402000  00002000
       000000000000000d  0000000000000000  WA       0     0     4
  [ 3] .symtab           SYMTAB           0000000000000000  00002010
       00000000000000f0  0000000000000018           4     6     8
  [ 4] .strtab           STRTAB           0000000000000000  00002100
       0000000000000031  0000000000000000           0     0     1
  [ 5] .shstrtab         STRTAB           0000000000000000  00002131
       0000000000000027  0000000000000000           0     0     1
Key to Flags:
  W (write), A (alloc), X (execute), M (merge), S (strings), I (info),
  L (link order), O (extra OS processing required), G (group), T (TLS),
  C (compressed), x (unknown), o (OS specific), E (exclude),
  l (large), p (processor specific)
```

Here we can see that the address of the `.text` and `.data` sections have been
set up to 0x401000 and 0x402000. Now if we take a look in the disassembly:

```shell
$ objdump -d -M intel hello

hello:     file format elf64-x86-64


Disassembly of section .text:

0000000000401000 <_start>:
  401000:       b8 01 00 00 00          mov    eax,0x1
  401005:       bf 01 00 00 00          mov    edi,0x1
  40100a:       48 be 00 20 40 00 00    movabs rsi,0x402000
  401011:       00 00 00
  401014:       ba 0d 00 00 00          mov    edx,0xd
  401019:       0f 05                   syscall
  40101b:       b8 3c 00 00 00          mov    eax,0x3c
  401020:       bf 00 00 00 00          mov    edi,0x0
  401025:       0f 05                   syscall
```

Look! The address in the `movabs` instruction, at offset `0xc` was updated
to `0x402000` (in little endian)!.

Another thing you may notice if you run `readelf hello -a` is that it now
contains a program header table:

```shell
$ readelf hello -l

Elf file type is EXEC (Executable file)
Entry point 0x401000
There are 3 program headers, starting at offset 64

Program Headers:
  Type           Offset             VirtAddr           PhysAddr
                 FileSiz            MemSiz              Flags  Align
  LOAD           0x0000000000000000 0x0000000000400000 0x0000000000400000
                 0x00000000000000e8 0x00000000000000e8  R      0x1000
  LOAD           0x0000000000001000 0x0000000000401000 0x0000000000401000
                 0x0000000000000027 0x0000000000000027  R E    0x1000
  LOAD           0x0000000000002000 0x0000000000402000 0x0000000000402000
                 0x000000000000000d 0x000000000000000d  RW     0x1000

 Section to Segment mapping:
  Segment Sections...
   00
   01     .text
   02     .data
```

A program header contains information about things that the system need to
prepared for the program execution. Here we have three headers of type `LOAD`
that tells which segments of the file need to be loaded in memory.

The first one tells to load 232 (`0xe8`) bytes starting at offset 0 of the file
(that is, the ELF header and program headers) with read permissions in virtual
address `0x40_0000` (or physical address `0x40_0000` but most PCs today uses
[virtual memory]). (I am not sure why the ELF header is loaded in memory.)

[virtual memory]: https://en.wikipedia.org/wiki/Virtual_memory

The second one loads the `.text` section, with read and exec permissions, and
the third loads `.data` with read and write permissions. 

The program headers themselves don't say which sections they are mapping
(although readelf shows the mapping), but you can check this yourself by
comparing the offset and address of the segments here, with the ones in the
section header of each section.

## A minimal hello world

Again, is always good to start experimenting with an idea by implementing the
simplest example that we can make. In our case that would be compiling a hello
world program. But the one that I showed is still not minimal enough.

Because our `'Hello World!\n'` string is only read during the execution, it does
not need to live in a section with read and write permission like the `.data`.
We could but it in a `.rodata`, which is read only, but we can take advantage
that the `.text` segment has read permission and put the string there, saving an
entire section:

```nasm
section .text
    global _start 

_start:
    mov rax, 1           ; 'write' system call = 4
    mov rdi, 1           ; file descriptor 1 = STDOUT
    lea rsi, [rel hello] ; string to write
    mov rdx, helloLen    ; length of string to write
    syscall              ; call the kernel

    ; Terminate program
    mov rax, 60          ; 'exit' system call = 60
    mov rdi, 0           ; exit with error code 0
    syscall              ; call the kernel
    hello:    db 'Hello world!',10
    helloLen: equ $-hello
```


And by putting the string alongside the code, we can make sure the string is
always at a constant offset from the code. This means that I could replace the
`mov` of the absolute address of `hello` in the code, by a `lea rsi, [rel
hello]` rip relative addressing, saving a relocation!

If you take a look at the disassembly now, you will see that that instruction is
compiled down to `lea rsi, [rip+0x13]`:

```shell
$ nasm hello.as -f elf64 -o hello.o
$ objdump -d -M intel hello.o

hello.o:     file format elf64-x86-64


Disassembly of section .text:

0000000000000000 <_start>:
   0:   b8 01 00 00 00          mov    eax,0x1
   5:   bf 01 00 00 00          mov    edi,0x1
   a:   48 8d 35 13 00 00 00    lea    rsi,[rip+0x13]        # 24 <hello>
  11:   ba 0d 00 00 00          mov    edx,0xd
  16:   0f 05                   syscall
  18:   b8 3c 00 00 00          mov    eax,0x3c
  1d:   bf 00 00 00 00          mov    edi,0x0
  22:   0f 05                   syscall

0000000000000024 <hello>:
  24:   48                      rex.W
  25:   65 6c                   gs ins BYTE PTR es:[rdi],dx
  27:   6c                      ins    BYTE PTR es:[rdi],dx
  28:   6f                      outs   dx,DWORD PTR ds:[rsi]
  29:   20 77 6f                and    BYTE PTR [rdi+0x6f],dh
  2c:   72 6c                   jb     9a <hello+0x76>
  2e:   64 21 0a                and    DWORD PTR fs:[rdx],ecx
```

(Also, these strange instruction are there only because objdump is trying to
interpret the string data as code now.)

# The object crate

Now we can finally start making a small compiler. First, lets use [dynasm], like
we did in part two, to get our hello world program in Rust:

[dynasm]:https://docs.rs/dynasmrt/1.2.3/dynasmrt/index.html

```rust
let mut code: VecAssembler<X64Relocation> = VecAssembler::new(0);

let hello_str = b"Hello world!\n";
let len = hello_str.len() as i32;
dynasm!(code
    ; mov eax,1            // 'write' system call = 4
    ; mov edi,1            // file descriptor 1 = STDOUT
    ; lea rsi, [>hello]    // string to write
    ; mov edx, DWORD len   // length of string to write
    ; syscall              // call the kernel

    // Terminate program
    ; mov eax,60           // 'exit' system call
    ; mov edi,0            // exit with error code 0
    ; syscall              // call the kernel
    ; hello:
    ; .bytes hello_str
);

let code = code.finalize().unwrap();
```

We can directly run that code using [memmap2], to make sure everything is
working:

[memmap2]: https://docs.rs/memmap2/0.5.8/memmap2/index.html

```rust
let mut buffer = memmap2::MmapOptions::new()
    .len(code.len())
    .map_anon()
    .unwrap();

buffer.copy_from_slice(code.as_slice());

let buffer = buffer.make_exec().unwrap();

let hello: unsafe extern "C" fn() -> ! = std::mem::transmute(buffer.as_ptr());
hello()
```

If you run this, it will print `Hello World!`.

But we should be compiling it to an object file. For this we will use the
[`object`] crate. As its docs says, "the `object` crate provides a unified
interface to working with objects files across platforms".

[`object`]: https://docs.rs/object/0.30.0/object/index.html

This crate will allow us to write our object files, without needing to worry
about the details of each object format, making it easier to make our compiler
cross-platform. It also allows writing ELF and PE executables directly, but we
would need to use its lower level API for each format, so we will not cover that
here.

So first we crate a `Object` struct with the specification of our host:

```rust
let mut obj = object::write::Object::new(
    object::BinaryFormat::Elf,
    object::Architecture::X86_64,
    object::Endianness::Little,
);
```

Next we can add the `_start` symbol to the object:

```rust
let start = obj.add_symbol(Symbol {
    name: b"_start".to_vec(),
    kind: object::SymbolKind::Text,
    scope: object::SymbolScope::Linkage,
    weak: false,
    flags: SymbolFlags::None,

    value: 0,
    size: 0,
    section: object::write::SymbolSection::Undefined,
});
```

We set its name; its kind, text, because it points to executable code; its
scope, linkage, which make the symbol global, like we did in the assembly; it
doesn't need to be weak (weak symbols can be overridden by non-weak symbols);
and it will need any special flags.

Value, size and section will be updated when we add data to the symbol, so I put
some default values there for now.

Now we can crate the `.text` section, and by calling `add_symbol_data` we append
our code to it and at same time we update the `_start` symbol.
x86 instructions don't need to be aligned, so the alignment is 1.

```rust
let text = obj.section_id(object::write::StandardSection::Text);
obj.add_symbol_data(start, text, &code, 1);
```

And that is it! Now we can emit it, and save it to a file!

```rust
let mut out = Vec::new();
obj.emit(&mut out).unwrap();

std::fs::write("hello.o", out).unwrap();
```

If we take a look at the object generated, we see that everything is as we
expected:

```shell
$ readelf -Srs hello.o
There are 5 section headers, starting at offset 0xd8:

Section Headers:
  [Nr] Name              Type             Address           Offset
       Size              EntSize          Flags  Link  Info  Align
  [ 0]                   NULL             0000000000000000  00000000
       0000000000000000  0000000000000000           0     0     0
  [ 1] .text             PROGBITS         0000000000000000  00000040
       0000000000000031  0000000000000000  AX       0     0     1
  [ 2] .symtab           SYMTAB           0000000000000000  00000078
       0000000000000030  0000000000000018           3     1     8
  [ 3] .strtab           STRTAB           0000000000000000  000000a8
       0000000000000008  0000000000000000           0     0     1
  [ 4] .shstrtab         STRTAB           0000000000000000  000000b0
       0000000000000021  0000000000000000           0     0     1
Key to Flags:
  W (write), A (alloc), X (execute), M (merge), S (strings), I (info),
  L (link order), O (extra OS processing required), G (group), T (TLS),
  C (compressed), x (unknown), o (OS specific), E (exclude),
  l (large), p (processor specific)

There are no relocations in this file.

Symbol table '.symtab' contains 2 entries:
   Num:    Value          Size Type    Bind   Vis      Ndx Name
     0: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT  UND
     1: 0000000000000000    49 FUNC    GLOBAL HIDDEN     1 _start
```

Now let's link and run it:

```shell
$ ld hello.o -o hello
$ ./hello
Hello world!
```

It works!

# A zero-dependency Static Compiler

Now we know everything necessary to make the first version of our brainfuck
compiler!

First, I will take an old version of our JIT compiler, the one right after [we
start using `dynasm`][previousJIT], while we were still using syscalls. This is
because now our compiled code will not have access to Rust functions (unless
we compile them to a library and link them, as we will see later).

[previousJIT]: {% post_url 2022-10-16-bf_compiler-part2 %}#improving-our-workflow-with-dynasm

We also will need to make some changes. In our JIT compiler we were passing the
program's memory as an argument to the compiled code, but we will not be able to
do this anymore. So instead we will allocate that memory on the stack.

To allocate memory from the stack is very easy, we only need to decrease the
value `rsp` by how much memory we need to allocate (the stack grows downwards).
We also need to make sure to clear the memory, because brainfuck programs assume
that they are initially zeroed[^actually_is_zeroed].

[^actually_is_zeroed]: Actually, on Linux, fresh allocated stack for a process
    is initially zeroed, and because we are the first function being executed (we
    are at the entry point) we can make sure that the memory is already clear. But
    I cleared it anyway, because it will be useful when we port this compiler to
    Windows.

Note that we are allocation a relative large value, 30000 bytes, but modern
desktop systems have at least 1 MiB of reserved space for the stack, so that
should not be as problem.

Another change is that we can no longer finish our program with a `ret`, because
there is no one to return to. So we replace it with a `exit` syscall.

So here is what the code becomes:

```rust
struct Program {
    code: Vec<u8>,
}
impl Program {
    fn new(source: &[u8]) -> Result<Program, UnbalancedBrackets> {
        let mut code: VecAssembler<X64Relocation> = VecAssembler::new(0);

        // r12 will be the adress of `memory`
        // r13 will be the value of `pointer`
        // r13 is set to 0
        dynasm! { code
            ; .arch x64
            ; push rbp
            ; mov rbp, rsp
            ; xor r13, r13

            // allocate 30_0000 bytes on stack for the memory
            ; sub rsp, 30_000
            ; mov r12, rsp

            // zero the memory
            ; xor eax, eax
            ; mov r11, r12
            ; loop_:
            ; mov QWORD [r11], rax
            ; add r11, 8
            ; cmp r11, rbp
            ; jne <loop_
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
                    ; mov rax, 1           // write syscall
                    ; mov rdi, 1           // stdout's file descriptor
                    ; lea rsi, [r12 + r13] // buf address
                    ; mov rdx, 1           // length
                    ; syscall
                },
                b',' => dynasm! { code
                    ; .arch x64
                    ; mov rax, 0           // read syscall
                    ; mov rdi, 0           // stdin's file descriptor
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
                _ => continue,
            }
        }

        if !bracket_stack.is_empty() {
            return Err(UnbalancedBrackets(']', code.offset().0));
        }

        dynasm! { code
            ; .arch x64
            ; xor rax, rax
            ; ->exit:
            ; mov rdi, rax // exit error code
            ; mov rax, 60  // exit syscall
            ; syscall
        }

        Ok(Program {
            code: code.finalize().unwrap(),
        })
    }
}
```

Now the only thing we need to do is to write it to an object file:

```rust
fn to_object(&self) -> Vec<u8> {
    let mut obj = object::write::Object::new(
        object::BinaryFormat::Elf,
        object::Architecture::X86_64,
        object::Endianness::Little,
    );

    let start = obj.add_symbol(Symbol {
        name: b"_start".to_vec(),
        value: 0,
        size: 0,
        kind: object::SymbolKind::Text,
        scope: object::SymbolScope::Linkage,
        weak: false,
        section: object::write::SymbolSection::Undefined,
        flags: SymbolFlags::None,
    });

    let text = obj.section_id(object::write::StandardSection::Text);
    obj.add_symbol_data(start, text, &self.code, 16);
    let mut out = Vec::new();
    obj.emit(&mut out).unwrap();

    out
}
```

And in the main:

```rust
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};
fn main() -> ExitCode {
    let mut args = std::env::args();

    let input_file = args.nth(1).unwrap();
    let source = match std::fs::read(&input_file) {
        Ok(x) => x,
        Err(err) => {
            eprintln!("Error reading '{}': {}", input_file, err);
            return ExitCode::from(2);
        }
    };

    let program = match Program::new(&source) {
        Ok(x) => x,
        Err(UnbalancedBrackets(c, address)) => {
            eprintln!(
                "Error parsing file: didn't found pair for `{}` at instruction index {}",
                c, address
            );
            return ExitCode::from(3);
        }
    };

    let file_name = Path::new(&input_file).file_name().unwrap();
    let output_file = PathBuf::from(&file_name).with_extension("o");

    let obj = program.to_object();
    std::fs::write(output_file, obj).unwrap();

    ExitCode::from(0)
}
```

And if we run:

```shell
$ cargo run -p singlepass-compiler -- programs/factor.bf
    Finished dev [unoptimized + debuginfo] target(s) in 0.03s
     Running `target/debug/singlepass-compiler programs/factor.bf`
$ ld factor.o -o factor
$ ./factor
123456
123456: 2 2 2 2 2 2 3 643
```

It works!

# Linking object files

Now let's take a look at linking multiple object files.

First let's modify our hello world example to instead of calling the `write`
syscall directly, it will call a function defined in another file. So we
declared that `my_write` is an external symbol, and call it, passing the string
address and length as argument:

```nasm
section .text
    global _start 
    extern my_write

_start:
    lea rdi, [rel hello] ; string to write
    mov rsi, helloLen    ; length of string to write
    call my_write

    ; Terminate program
    mov rax, 60          ; 'exit' system call = 60
    mov rdi, 0           ; exit with error code 0
    syscall              ; call the kernel
    hello:    db 'Hello world!',10
    helloLen: equ $-hello
```

And in a file called `write.as`, we declared `my_write` as global:

```nasm
section .text
    global my_write

my_write:
    push rbp        ; push rbp to keep the stack 16 bytes aligned, and
    mov rbp, rsp    ; also setup the stack frame.

    mov rdx, rsi    ; length of string to write
    mov rsi, rdi    ; string to write
    mov rax, 1      ; 'write' system call = 4
    mov rdi, 1      ; file descriptor 1 = STDOUT
    syscall         ; call the kernel

    pop rbp
    ret
```

If we assemble and link everything, we see that it continues to work:

```shell
$ nasm hello.as -f elf64 -o hello.o
$ nasm write.as -f elf64 -o write.o
$ ld hello.o write.o -o hello
$ ./hello
Hello world!
```

Now if we take a look at `hello.o` we will see that it now contains relocations:

```shell
$ readelf -r hello.o

Relocation section '.rela.text' at offset 0x300 contains 1 entry:
  Offset          Info           Type           Sym. Value    Sym. Name + Addend
00000000000d  000500000002 R_X86_64_PC32     0000000000000000 my_write - 4
```

The relocation is of type `R_X86_64_PC32`, which means that it writes a 32-bit
word whose value is the offset from the relocation to the given symbol. This is
because the call instruction receives an offset as argument.

But the call instruction receives an offset relative to the address of the next
instruction, so instead of writing `S - P` (symbol minus relocation address) it
should be written `S - (P + 4)`, i.e., `S - P - 4`. So the relocation has an
addend of `-4`.

So let's translate that new version of the code to our example compiler. The
code now is:

```rust
let mut code: VecAssembler<X64Relocation> = VecAssembler::new(0);

let hello_str = b"Hello world!\n";
let len = hello_str.len() as i32;

let relocation_offset;
dynasm!(code
    ; lea rdi, [>hello]    // string to write
    ; mov rsi, DWORD len   // length of string to write
    ; call DWORD 0
    ;; relocation_offset = code.offset().0 as u64 - 4

    // Terminate program
    ; mov eax,60           // 'exit' system call
    ; mov edi,0            // exit with error code 0
    ; syscall              // call the kernel
    ; hello:
    ; .bytes hello_str
);

let code = code.finalize().unwrap();
```

As the address of `my_write` is unknown at this point, the call instruction
receives a dummy value for now and the offset where the relocation will take
place is saved in `relocation_offset`.

So, we when we are building the `Object`, we add the `my_write` symbol, and the
relocation. The new symbol is mostly equal to the other one, the only difference
is the name, and that we will not call `add_symbol_data` to it. 

The relocation will have size of 32bits; its kind is `Relative`; in this
particular case the encoding don't change anything, so I set it to `Generic`;
and I make sure to set the `addend` to `-4`, as we have seen previously:

```rust
let my_write = obj.add_symbol(Symbol {
    name: b"my_write".to_vec(),
    value: 0,
    size: 0,
    kind: object::SymbolKind::Text,
    scope: object::SymbolScope::Linkage,
    weak: false,
    section: object::write::SymbolSection::Undefined,
    flags: SymbolFlags::None,
});

let text = obj.section_id(object::write::StandardSection::Text);
obj.add_relocation(
    text,
    Relocation {
        offset: relocation_offset,
        size: 32,
        kind: object::RelocationKind::Relative,
        encoding: object::RelocationEncoding::Generic,
        symbol: my_write,
        addend: -4,
    },
)
.unwrap();
```

If we run and link to `write.o` we see that it works!

```shell
$ cargo run -p object-example -- obj
    Finished dev [unoptimized + debuginfo] target(s) in 0.03s
     Running `../target/debug/object-example obj`
$ ld hello.o write.o
$ ./hello
Hello world!
```

But now, let's try, instead of linking to an object written in assembly, link to
a static lib written in Rust.

For this, let's rewrite `write.as` as `write.rs`:

```rust
#[no_mangle]
pub extern "sysv64" fn my_write(address: *const u8, len: usize) {
    let string = unsafe { std::slice::from_raw_parts(address, len) };
    let string = std::str::from_utf8(string).unwrap();
    print!("{}", string);
}
```

We need to add the attribute `no_mangle` to make sure that `rustc` don't
[mangle] the name of the symbol for this function.

[mangle]: https://en.wikipedia.org/wiki/Name_mangling#Rust

So we can compile the code to a static library using `rustc` (I didn't bother
creating an entire cargo project for a single function):

```shell
$ rustc --crate-type=staticlib write.rs
```

This produces a `libwrite.a`, which is a static library. Static libraries are
basically an archive containing multiple object files (you can list them by
running `ar -t libwrite.a`).

Now we can try linking the library with our compiler object (make sure to put
object files before the libraries that they depend on):

```shell
$ ld hello.o libwrite.a
```

If run the command above, you will notice that it will output almost 3000 lines
of "undefined symbol" errors. That is because the Rust library that we build is
static linked to the Rust standard library, that in turn depends on some
libraries provided by the OS.

One way of fixing the error is by finding all libraries that define all the
missing symbols, and add them as argument to the linker. This is kinda trick, I
am not even sure if this would work, and it will vary between Linux distros, so
let's instead use `gcc` to link everything. It will already take care of linking
all system libraries.

The std also needs some extra static libraries. To get a list of them you can
pass `--print=native-static-libs` to the rustc invocation:

```shell
$ rustc --crate-type=staticlib write.rs --print=native-static-libs
note: Link against the following native artifacts when linking against this static library. The order and any duplication can be significant on some platforms.

note: native-static-libs: -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc

$ gcc hello.o libwrite.a -o hello -nostartfiles -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc
$ ./hello
Hello world!
```

And it works! Note that I need to pass `-nostartfiles`, because I am declaring
`_start` directly, instead of declaring a `main` function that would be called
by the C runtime.

# A Static Compiler

Now we can make the final version of our single-pass compiler. First, we create a
library with all functions that we will use in our compiled brainfuck programs.

We will put them in a file called `bf_lib.rs`. There will be our `read` and
`write` function, and also a function for exiting. Also, these function will not
return errors, because there is no one to report them like in our JIT version.
Instead, they only print the error message and exit.

I will also prefix each function with `bf_` to avoid symbol conflicts:

```rust
use std::io::{Read, Write};

#[no_mangle]
pub extern "sysv64" fn bf_write(value: u8) {
    // Writing a non-UTF-8 byte sequence on Windows error out.
    if cfg!(target_os = "windows") && value >= 128 {
        return;
    }

    let mut stdout = std::io::stdout().lock();

    let result = stdout.write_all(&[value]).and_then(|_| stdout.flush());

    if let Err(err) = result {
        eprintln!("IO error: {}", err);
        std::process::exit(1);
    }
}

#[no_mangle]
pub unsafe extern "sysv64" fn bf_read(buf: *mut u8) {
    let mut stdin = std::io::stdin().lock();
    loop {
        let mut value = 0;
        let err = stdin.read_exact(std::slice::from_mut(&mut value));

        if let Err(err) = err {
            if err.kind() != std::io::ErrorKind::UnexpectedEof {
                eprintln!("IO error: {}", err);
                std::process::exit(1);
            }
            value = 0;
        }

        // ignore CR from Window's CRLF
        if cfg!(target_os = "windows") && value == b'\r' {
            continue;
        }

        *buf = value;
        break;
    }
}

#[no_mangle]
pub unsafe extern "sysv64" fn bf_exit() {
    std::process::exit(0);
}
```

Now in our compiler, we will call these functions instead of the syscall. We
need to register the relocations for each call.

So `Program` becomes:

```rust
struct Program {
    code: Vec<u8>,
    write_relocations: Vec<usize>,
    read_relocations: Vec<usize>,
    exit_relocation: usize,
}
```

And the compilation:

```rust
let mut write_relocations = Vec::new();
let mut read_relocations = Vec::new();
```

```rust
b'.' => dynasm! { code
    ; .arch x64
    ; mov rdi, [r12 + r13] // cell value
    ; call DWORD 0
    ;; write_relocations.push(code.offset().0 - 4)
},
b',' => dynasm! { code
    ; .arch x64
    ; lea rdi, [r12 + r13] // cell address
    ; call DWORD 0
    ;; read_relocations.push(code.offset().0 - 4)
},
```

```rust
let exit_relocation;

dynasm! { code
    ; .arch x64
    ; call DWORD 0
    ;; exit_relocation = code.offset().0 - 4
}

Ok(Program {
    code: code.finalize().unwrap(),
    write_relocations,
    read_relocations,
    exit_relocation,
})
```

And we make sure to write these relocations on the generated object:

```rust
fn to_elf_object(&self) -> Vec<u8> {
    let mut obj = object::write::Object::new(
        object::BinaryFormat::Elf,
        object::Architecture::X86_64,
        object::Endianness::Little,
    );

    let mut add_symbol = |name: &[u8]| {
        obj.add_symbol(Symbol {
            name: name.to_vec(),
            value: 0,
            size: 0,
            kind: object::SymbolKind::Text,
            scope: object::SymbolScope::Linkage,
            weak: false,
            section: object::write::SymbolSection::Undefined,
            flags: SymbolFlags::None,
        })
    };

    let start = add_symbol(b"_start");
    let bf_write = add_symbol(b"bf_write");
    let bf_read = add_symbol(b"bf_read");
    let bf_exit = add_symbol(b"bf_exit");

    let text = obj.section_id(object::write::StandardSection::Text);
    obj.add_symbol_data(start, text, &self.code, 16);

    let mut add_call_reloc = |offset, symbol| {
        obj.add_relocation(
            text,
            Relocation {
                offset: offset as u64,
                symbol,
                size: 32,
                kind: object::RelocationKind::Relative,
                encoding: object::RelocationEncoding::Generic,
                addend: -4,
            },
        )
        .unwrap();
    };

    for offset in self.read_relocations.iter().copied() {
        add_call_reloc(offset, bf_read);
    }
    for offset in self.write_relocations.iter().copied() {
        add_call_reloc(offset, bf_write);
    }
    add_call_reloc(self.exit_relocation, bf_exit);

    let mut out = Vec::new();
    obj.emit(&mut out).unwrap();

    out
}
```

Now if we compile everything:

```shell
$ cargo run -p singlepass-compiler -- ../programs/factor.bf
    Finished dev [unoptimized + debuginfo] target(s) in 0.03s
     Running `../target/debug/singlepass-compiler programs/factor.bf`
$ rustc --crate-type=staticlib bf_lib.rs
$ gcc factor.o libbf_lib.a -nostartfiles -pthread -ldl -o factor
$ ./factor
123456
123456: 2 2 2 2 2 2 3 643
```

It works! Cool!

# A Windows Port

Following the spirit of the precious parts, let's now try porting our compiler
to Windows!

At first, it is very straightforward. In the compiler, we need to tell `object` to
crate a COFF object file on Windows, and we also need to replace the symbol
`_start` by `WinMain`, which is the one used as entry point on Windows.

```rust
let (format, entry_name) = if cfg!(target_os = "windows") {
    (object::BinaryFormat::Coff, "WinMain")
} else if cfg!(target_os = "linux") {
    (object::BinaryFormat::Elf, "_start")
} else {
    unimplemented!("Only Linux and Windows are implemented")
};
let entry_name = entry_name.as_bytes();

let mut obj = object::write::Object::new(
    format,
    object::Architecture::X86_64,
    object::Endianness::Little,
);

let mut add_symbol = |name: &[u8]| { /* ... */ };

let start = add_symbol(entry_name);
// ...
```

Then, if we use the `x86_64-pc-windows-gnu` target, we can continue to link our
objects with GCC:

```shell
> rustc --crate-type staticlib bf_lib.rs --target=x86_64-pc-windows-gnu
> cargo run -p singlepass-compiler -- ../programs/factor.bf
    Finished dev [unoptimized + debuginfo] target(s) in 0.17s
     Running `..\target\debug\singlepass-compiler.exe ../programs/factor.bf`
> gcc -o factor.exe factor.o libbf_lib.a -nostartfiles -ladvapi32 -luserenv -lkernel32 -lkernel32 -lws2_32 -lbcrypt
> factor.exe

```

If you test the executable now, you may notice that nothing happens. And if you
open it on a debugger, you will notice that it is actually crashing on an access
violation when clearing the first byte of the memory.

What is happening here is that, on Windows, only part of the reserved stack
space is actually committed. When I tested it, for example, there were only
12 KiB of accessible space on the stack.

But at the end of the committed stack, there are [guard pages], that whenever
they are accessed, more pages are allocated for the stack. Pages have a size of
4096 bytes, but we are allocating 30000 bytes on the stack, which is causing us
to miss the guard page, and access uncommitted memory.

[guard pages]: https://devblogs.microsoft.com/oldnewthing/20220203-00/?p=106215

The fix for this is very simple, we only need to make sure to not miss the guard
page. An easy change is to start clearing our memory at the bottom end of the
stack:

```rust
dynasm! { code
    ; .arch x64
    ; push rbp
    ; mov rbp, rsp
    ; xor r13, r13

    // allocate 30_0000 bytes on stack for the memory
    ; sub rsp, 30_000
    ; mov r12, rsp

    // zero the memory
    ; xor eax, eax
    ; mov r11, rbp
    ; loop_:
    ; add r11, -8
    ; mov QWORD [r11], rax
    ; cmp r11, r12
    ; jne <loop_
};
```

If we test our program now:

```shell
> rustc --crate-type staticlib bf_lib.rs --target=x86_64-pc-windows-gnu
> cargo run -p singlepass-compiler -- ../programs/factor.bf -o
    Finished dev [unoptimized + debuginfo] target(s) in 0.17s
     Running `..\target\debug\singlepass-compiler.exe ../programs/factor.bf`
> gcc -o factor.exe factor.o libbf_lib.a -nostartfiles -ladvapi32 -luserenv -lkernel32 -lkernel32 -lws2_32 -lbcrypt
> factor.exe
123456
123456: 2 2 2 2 2 2 3 643
```

It works!

We can also use the `msvc` tool chain. If we open a "Developer Command Prompt
for VS 22" (I have the Visual Studio 2022 installed), we can run the following:

```shell
> rustc --crate-type staticlib bf_lib.rs --target=x86_64-pc-windows-msvc -Copt-level=2 -Clto -Cpanic=abort
> link /subsystem:console /entry:WinMain advapi32.lib advapi32.lib userenv.lib kernel32.lib kernel32.lib ws2_32.lib bcrypt.lib msvcrt.lib vcruntime.lib factor.o bf_lib.lib
Microsoft (R) Incremental Linker Version 14.33.31629.0
Copyright (C) Microsoft Corporation.  All rights reserved.

bf_lib.lib(bf_lib.bf_lib.58e9fd71-cgu.2.rcgu.o) : warning LNK4210: .CRT section exists; there may be unhandled static initializers or terminators
msvcrt.lib(tlssup.obj) : warning LNK4210: .CRT section exists; there may be unhandled static initializers or terminators
bf_lib.lib(bf_lib.bf_lib.58e9fd71-cgu.2.rcgu.o) : warning LNK4210: .CRT section exists; there may be unhandled static initializers or terminators
msvcrt.lib(tlssup.obj) : warning LNK4210: .CRT section exists; there may be
unhandled static initializers or terminators

> factor.exe
123456
123456: 2 2 2 2 2 2 3 643
```

I needed to include `vcruntime.lib`, even though `--print=static-native-libs`
didn't report that it was used. Even so, there were some unresolved symbols, but
thankfully by setting `panic=abort`, enabling LTO and increasing the
optimization level, these symbols have gone away.

It still outputs some warnings about unhandled static initializers, but
thankfully our program don't need them (I think), and everything works!

# Future work

In this post we have seen the basic of object files, and how compile our
programs to them, and them link them. But we only covered the very basic of
this. There still things that we didn't cover, like dynamic libraries, position independent
executables, debug information, etc.

Also, all these file formats are system dependent, and the relocations in them
vary greatly between architectures, so we again fall in the same problem that we
discussed in the end of part 2. Thankfully code generators, like LLVM, also
covers these problems.

A natural next step here would be to try exactly that using the Cranelift JIT
compiler that we made in the last part as basis. You can use
[`cranelift-module`] and [`cranelift-object`] to do that.

[`cranelift-module`]: https://docs.rs/cranelift-module/latest/cranelift_module/
[`cranelift-object`]: https://docs.rs/cranelift-object/latest/cranelift_object/

But this will be the last part of this blog post series. There is much more to
explore on compilation, but brainfuck is too simple a language for them to be
need.

But hopefully, these series has taught you at least the basics to let you be
able to start writing your own compilers if the will ever came.


All the code developed during these series [can be found here][the_code] (may
not be exactly equal to one showed here).

[the_code]: https://github.com/Rodrigodd/bf-compiler
