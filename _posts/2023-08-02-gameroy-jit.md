---
layout: post
title:  "JIT compilation in a high accurate Game Boy emulator"
date:   2023-08-02 16:50:00 -0300
---

In the past two years, I spend a lot of time working in my Game Boy emulator. It
reached a good point, where it passes a lot of tests (comparable to some of the most
accurate emulators), have a GUI interface with a debugger and a disassembler.

But one thing that I always wanted to do, even before developing this emulator,
was to implement a dynamic recompiler, a.k.a. a JIT compiler. Since the first
time that I search how an emulator works, and one of the first descriptions I
found was about the difference between interpreters and dynamic recompilers,
which made me very interested in the subject.

Reinspired by JIT compilers (like ones in Java, JavaScript and LuaJIT), by
Dolphin's development posts, and mainly by [this blog post by Brook
Heisler][nes_jit] about a JIT compiler for NES, I started implementing one in my
emulator.

[nes_jit]: https://bheisler.github.io/post/experiments-in-nes-jit-compilation/

And one interesting point in Brook Heisler's post was about a limitation in how
it handled interrupts in the compiled code. When developing his JIT compiler, he
encountered a trade-off between the performance and precision on how to handle
interrupts in his emulator. He ultimately decided to sacrifice performance for
precision, but I was not content with that.

After thinking for some time, I come up with a solution to this problem,
a way to achieve maximum performance without sacrificing precision.

In this blog post, I will describe the process and considerations of
implementing a JIT compiler in my emulator, and how I solved the problem of
handling interrupts.

Also note that I am presenting the subjects in the order they came up in my
thought process, which just happens to be the reverse order in which they were
implemented (except the interpreter).

## Interpreter x Just-In-Time compilation

An emulator has the job of reproducing the behavior of a computer system in
software. A computer system, including the Game Boy, is composed of many
components, and all of them need to be emulated. But the most important (or at
least the most interesting) component is the CPU, responsible for directly
executing the instructions of the programs that you pretend to run in your
emulator.

There are two main ways of emulating the CPU (or any programmable component).

### A very basic interpreter

The easiest way of emulating the CPU of some system is by implementing an
interpreter. Basically the interpreter, like the CPU, reads the instructions
from the emulated memory, decoded it, and emulate its execution by updating the
emulator state.

In the case of the Game Boy, the instruction to be executed is read, byte by
byte (Game Boy is an 8-bit system), at the memory address pointed by the program
counter (PC), where after each read the PC is incremented. To decode the
instruction only the first byte is need for most instructions, and because there
is only 255 values for a byte, we can just use a look-up table. The Game Boy CPU
also have 0xCB prefixed instructions that are decoded from the second byte,
using another look up table.

I am using Rust for implementing my emulator, so we can use a `match` statement
for decoding the instruction. Each arm of the `match` lead to a function
implementing the execution of the instruction. This function will read and write
to registers or memory, reading more bytes from the instructions if necessary
(like for immediate values, the address of jump instruction for example).

[^simplified_code]: The actual code has a lot more abstraction, and do things
    like proper wrapping operations.

A very simplified[^simplified_code] code for the interpreter would be something like this:

```rust
fn interpret_instruction(gb: &mut GameBoy) {
    let opcode = gb.read(gb.cpu.pc);
    gb.cpu.pc += 1;
    match opcode {
        0x00 => nop(gb),
        0x01 => load_bc_im16(gb),
        0x02 => load_bc_a(gb),
        0x03 => inc_bc(gb),
        0x04 => inc_b(gb),
        // 252 more arms like that...
    }
}

// ...

// INC B 1:4 Z 0 H -
fn inc_b(gb: &mut GameBoy) {
    gb.cpu.b += 1;    // increase B
    gb.cpu.f &= 0x1F; // clear Z, N, H flags
    if gb.cpu.b == 0 { gb.cpu.f |= 0x80; }       // set Z on zero
    if gb.cpu.b & 0xF == 0 { gb.cpu.f |= 0x80; } // set H on half carry
}
```

The `interpret_instruction` function is called in a loop, where each iteration
executes one instruction.

One problem with this approach is that the speed in which the interpreter can
execute the code is very behind the theoretical speed that yours computer CPU
execute the code.

The interpreter need to used multiple instructions of the host CPU to not only
execute the instruction, but also to read from memory and to decode it, steps
that the host CPU do in a single instruction. Not only that, the interpreter
needs to decode the same instruction multiple times.

But that is not a problem, at least not for the Game Boy, whose CPU runs with a
clock speed of just a little more than 1 MHz, while you may be reading this
in a device with much more than 1 GHz. In fact, my emulator can run Game Boy
games more than 60 times the original system speed in my notebook intel i5-4200U
2.6 GHz.

Well, but this is still 41 times slower than the theoretical speed that my CPU
can execute the code! If you assume the same amount of instructions per
instruction, not very reasonable, actually. But there is a very interesting
technique that can be used to improve the performance of an emulator:
**recompilation**.

### A very basic JIT compiler

The second way of emulating the CPU is by translating the instructions of the
Game Boy CPU to the instructions of the host CPU, and then executing it, which
is called **recompilation**. In the case of the Game Boy, and in many other
systems, you can not recompiled all the code containing in a program beforehand
(static recompilation), so you need to do that translation at runtime, which
make the technique be called **dynamic recompilation**, or, more commonly called,
**Just-In-Time compilation** (or JIT).

For that, you need to make a compiler. The compilation process is very similar
to the interpreter, where you read, decode and execute instruction, but instead
of executing the instruction directly, it will generate the machine code that
will.

The machine code can be generated just once and then executed multiple times,
bypassing the overhead of reading and decoding instructions of the interpreter.

In simplified form, the code for the compiler would be something like this:

```rust
fn compile_instruction(ctx: &mut JitCompiler, gb: &GameBoy, code: &mut Vec<u8>) {
    let opcode = gb.read(ctx.curr_pc);
    match opcode {
        0x00 => nop(ctx, gb),
        0x01 => load_bc_im16(ctx, gb),
        0x02 => load_bc_a(ctx, gb),
        0x03 => inc_bc(ctx, gb),
        0x04 => inc_b(ctx, gb),
        // 252 more arms like that...
    }
}

// ...

// INC B 1:4 Z 0 H -
fn inc_b(_: &mut JitCompiler, code: &mut Vec<u8>) {
    code.extend(&[
        0x0f, 0xb6, 0x47, 0x01, // movzx  eax,BYTE PTR [rdi+0x1] ; read F
        0x0f, 0xb6, 0x4f, 0x02, // movzx  ecx,BYTE PTR [rdi+0x2] ; read B
        0xfe, 0xc1,             // inc    cl                     ; increase B
        0x88, 0x4f, 0x02,       // mov    BYTE PTR [rdi+0x2],cl  ; write B
        0x24, 0x1f,             // and    al,0x1f                ; clear Z, N, H flags
        0x8d, 0x50, 0x80,       // lea    edx,[rax-0x80]
        0x0f, 0xb6, 0xc0,       // movzx  eax,al
        0x0f, 0xb6, 0xd2,       // movzx  edx,dl
        0x0f, 0x45, 0xd0,       // cmovne edx,eax
        0x8d, 0x42, 0x20,       // lea    eax,[rdx+0x20]
        0xf6, 0xc1, 0x0f,       // test   cl,0xf
        0x0f, 0xb6, 0xc0,       // movzx  eax,al
        0x0f, 0x45, 0xc2,       // cmovne eax,edx
        0x88, 0x47, 0x01,       // mov    BYTE PTR [rdi+0x1],al  ; write F
    ]);
}
```

This `compile_instruction` function reads and decodes an instruction, and then
write the machine code that executes that instruction to a buffer.

_(For the curious, I got that assembly/machine code by just [implementing the
instruction in Rust][inc_b_playground], and then disassembled it with `objdump
-d`.)_

[inc_b_playground]: https://play.rust-lang.org/?version=stable&mode=release&edition=2021&gist=77582b2e84dd1896f5c144a790d84e8e

So to make a JIT compiler, you just need to run `compile_instruction` to the
sequence of instructions starting at whatever address `PC` currently is, write
the code to a memory page with executable permission (you can use a crate like
[`memmap2`](https://crates.io/crates/memmap2) for that), transmute it to a
function pointer, and call it! Simple as that!

Well, actually not, some instructions that involve functions calls in its
implementation or do branching are much tricker to implement than just appending
a static array of bytes to a buffer, and you also need to make sure that the
code for each instruction can work together, using the same registers and such.
You also need a prologue and epilogue for the function that satisfy a [calling
convention](https://en.wikipedia.org/wiki/Calling_convention).

But this already shows the basics of a JIT compiler.

Also, in my actual implementation I used the crate [`dynasm`][dynasm] for
emitting the machine code (this crate has a macro that translate assembly code
to machine code at compile time), so the code is a lot more maintainable than the
array of raw bytes that I showed here.

[dynasm]: https://crates.io/crates/dynasm

## Optimizations

The cool thing about compiling code, is that you can do optimizations! For
example, one of the (few) optimizations that I make in the JIT compiler for my
emulator is the omission of unnecessary flag calculations (directly inspired by
the one described in [Brook Heisler's post][nes_jit]).

If you tried to read what the assembly code is doing, you may have noticed that
only instructions are used for increment the value of register B, and the
remaining 10 are being used for updating the conditional flags in the register
F. But the value of F may be overwritten by the next instructions before it
could be read from a conditional jump instruction for example. In that scenario,
updating the value of F does not affect the behavior of the emulation, so it is
a waste to compute.

If we compiled all these instructions together, we can detect that kind of
scenario, and omitted the flag calculation, greatly reducing the amount of code
emitted!

```rust
// INC B 1:4 Z 0 H -
fn inc_b(ctx: &mut JitCompiler, code: &mut Vec<u8>) {
    if !ctx.flag_is_need {
        code.extend(&[0xfe, 0x47, 0x02]); // inc BYTE PTR [rdi+0x2]
        return;
    }

    code.extend(&[
        0x0f, 0xb6, 0x47, 0x01, // movzx  eax,BYTE PTR [rdi+0x1] ; read F
        0x0f, 0xb6, 0x4f, 0x02, // movzx  ecx,BYTE PTR [rdi+0x2] ; read B
        0xfe, 0xc1,             // inc    cl                     ; increase B
        0x88, 0x4f, 0x02,       // mov    BYTE PTR [rdi+0x2],cl  ; write B
        0x24, 0x1f,             // and    al,0x1f                ; clear Z, N, H flags
        0x8d, 0x50, 0x80,       // lea    edx,[rax-0x80]
        0x0f, 0xb6, 0xc0,       // movzx  eax,al
        0x0f, 0xb6, 0xd2,       // movzx  edx,dl
        0x0f, 0x45, 0xd0,       // cmovne edx,eax
        0x8d, 0x42, 0x20,       // lea    eax,[rdx+0x20]
        0xf6, 0xc1, 0x0f,       // test   cl,0xf
        0x0f, 0xb6, 0xc0,       // movzx  eax,al
        0x0f, 0x45, 0xc2,       // cmovne eax,edx
        0x88, 0x47, 0x01,       // mov    BYTE PTR [rdi+0x1],al  ; write F
    ]);
}
```

Fantastic! The optimized version is 14 times smaller than the unoptimized one!

There are, of course, many other optimizations that can be done. That could
reach the point where the optimizations become by far the most complex part of
a compiler. Instead of implementing them your self, you could use a third party
compiler infrastructure, like [LLVM][llvm] or [Cranelift][clift], which
implement many optimizations for you.

[llvm]: https://llvm.org/
[clift]: https://cranelift.dev/

But my emulator is, at least for now, only targeting `x86-64` machine code, so
I will refrain from using any of them.

But there is a big problem in trying to apply any non-trivial optimization, even
the simple flag omission one: the fact that the CPU need to handle interrupts.

## Interrupts

Interrupts is a mechanism that allows the CPU to stop the execution of the
current code, and start executing a function that handles the interrupt. For
example, in the Game Boy, you can use the interrupt triggered by the VBlank of
PPU (which happens right after the screen is fully rendered) to update the game
logic and prepare the next frame for rendering.

There are [multiple sources of
interrupts](https://gbdev.io/pandocs/Interrupt_Sources.html) in the Game Boy,
which are triggered by one of the various components. So to handle the interrupts we
also need to update them.

And if you want a precise enough emulator, you need to update the components and
check for interrupts before every single instruction. And not only that, if you
pursue cycle-accuracy, you also need to update the components before each memory
access. One way of doing that is by using a `tick` function to do that.

Our interpreter becomes something like this:

```rust
fn interpret_instruction(gb: &mut GameBoy) {
    if gb.interrupts_enabled & gb.interrupts_flags != 0 {
        // this will write to the stack, and change PC to the interrupts handler
        handle_interrupt(gb);
    }

    let opcode = gb.read(gb.cpu.pc);
    gb.cpu.pc += 1;
    tick(gb, 4); // each access to memory takes 4 cycles
    match opcode {
        // ...
        0x36 => ld_hl_im8(gb),
    }
}

fn tick(gb: &mut GameBoy, cycles: u64) {
    gb.clock_count += cycles;

    gb.update_timer(clock_count);
    gb.update_ppu(clock_count);
    gb.update_sound_controller(clock_count);
    gb.update_serial(clock_count);
}

// LD (HL),d8 2:12 - - - -
fn ld_hl_im8() {
    let value = gb.read(gb.cpu.pc);
    gb.cpu.pc += 1;
    tick(gb, 4);

    let address = gb.cpu.h as u16 * 0x100 + gb.cpu.l as u16;
    gb.write(address, value);
    tick(gb, 4);
}
```

Ignoring all the complexity of emulating each component, and some quirks with
the interrupt handling timing, this is roughly how the interpreter handles the
interrupts.

But now, handling interrupts is a problem for our JIT compiler. Not only
constantly checking for interrupts may introduce a considerable amount of
overhead, we can no longer apply many of the optimizations that we would like to
do, like the flag calculation omission, because the code need to always be
prepared to branch to the interrupt handler.

You could try to only check for interrupts every 10 instructions or something,
it may be precise enough for most games (but not all). But my emulator pursues
maximum accuracy, and the point of the JIT compiler is to check if it is
possible to implement it, without sacrificing precision nor performance.

And I figured out a way to achieve that: by estimating when the next interrupt
will happen.

## Estimating the next interrupt

The JIT compiler in my emulator works this way: given the current address in the
register PC and the bank currently mapped to that address, the compiler produces
a block of instructions starting at that address, that is then compiled down to
machine code, and stored in a cache.

But before executing the block, it checks if an interrupt will happen during its
execution. If not, the block is executed, otherwise, we fall back to the
interpreter.

This allows us to apply all the optimizations that we want, without worrying
with interrupts (most of the part).

The disadvantage is that we still need to have an interpreter around, and the
fallback may decrease the emulator performance. But I already had an interpreter
working, so no problem in that, and only a small fraction of the instructions
need to be interpreted, so the loss of performance is not so bad. But an
alternative is to use as fallback a version of the compiler that handles
interrupts.

The big problem in this approach is estimating when the next interrupt will
happen. Depending on how a component generates interrupts, it may just not be
possible to estimate when the next interrupt will happen more efficiently than
just emulating the component until it does.

Thankfully, in the case of the components of the Game Boy, it is possible to
estimate the instant of the next interrupt in constant time. But implementing
the estimation is still tricky, with a lot of edge cases. But thanks to a lot
[fuzzing based testing](https://en.wikipedia.org/wiki/Fuzzing), I am somewhat
confident that I got it right.

If you want to take a look at how the implementation looks like, here are
links for the [timer][timer_next_interrupt], for the [PPU][ppu_next_interrupt],
and for the [serial][serial_next_interrupt].

[timer_next_interrupt]: https://github.com/Rodrigodd/gameroy/blob/4ac46fff3a203c20aadbb108585354701a52546e/core/src/gameboy/timer.rs#L262-L306
[ppu_next_interrupt]: https://github.com/Rodrigodd/gameroy/blob/4ac46fff3a203c20aadbb108585354701a52546e/core/src/gameboy/ppu.rs#L1536-L1723
[serial_next_interrupt]: https://github.com/Rodrigodd/gameroy/blob/4ac46fff3a203c20aadbb108585354701a52546e/core/src/gameboy/serial_transfer.rs#L121-L131

## Lazy Update

And with that we fix the problem with interrupts! But we still need to update
the components. But thankfully we only need to update the components before
accessing a memory address that is mapped to a component, or right before an
interrupt is triggered. So we can just update the components in the memory access
function, right before reading or writing to memory, and before checking for
interrupts in the interpreter.

To implement that, each component will contain a clock count of the last time
that it was updated, and a function that updates the component to the current
clock count. The cool part is that this allows us to further optimize the
emulation of each component by decreasing the number of times we update each
component, and because we can emulate many cycles at once in a more efficient
way.

The implementation of the PPU is a good example of that. The PPU is the
component responsible for transforming the tile map and sprites data into pixels
for the screen. The final rendering result is somewhat straightforward to
emulate if the input data don't change, but the necessity of taking into account
that the CPU may be changing the input during rendering, make it necessary to
emulate the [complex inner workings of the PPU](https://gbdev.io/pandocs/pixel_fifo.html).

But when we lazy update the PPU we can be sure that there were no changes since
the last update, allowing us to bypass most of the complexity of the PPU, and
just emulate the final rendering result, which is much faster.

In the figure below we can view the time spend emulating the PPU, when updating
the PPU every instruction, and when updating the PPU only when necessary.

![PPU emulation time](/assets/gameroy_jit/interrupt-prediction.svg)

The `draw_scan_line` function render the final result of a scan line, without
emulating the entire PPU inner workings. As it is possible to see, the emulation of
the PPU gains a speed-up of almost 10 times, increasing the overall speed of the
emulator by 4.

One thing to notice is that, before this optimization, only 14% of the time was
spent in the CPU. That means that the CPU emulation was hardly the bottleneck of
the emulator, and that the JIT compiler would not provide a significant
speed-up. But after this optimization, the CPU emulation is responsible for 63%
of the time, giving some room for the JIT compiler to shine.

## The Compiler a little more in depth

Above, I just provided a simplified version of how a JIT compiler would work.
But the actual implementation concerns itself with more details.

First, it needs to decided which group of instructions it will compile. For that
I have a step for "tracing a block", which basically, starting at a given
address (very likely the current address in the PC register), it starts decoding
instructions one after the other, until it reaches an unconditional branch (like
a jump or a call). This produces a block for that address.

Currently, the blocks are just a linear sequence of instructions, but I could
very well also expand the block tracing to follow branches, and potentially
compile big sections of the program at once. But my block caching is still
too suboptimal for that, as I will explain later.

An important point is that I only trace blocks when the PC is pointing to an
address in the memory ROM (range 0x0000 to 0xFFFF). If it is pointing outside of
it, like the RAM, I just fall back to the interpreter. This allows me to avoid
handling self modifying code, which is a general problem for dynamic
recompilers. Also, games in general don't run much code in RAM, so is not that
big of a loss.

Another point is that the Game Boy uses bank switching, which means that address
in the ROM range may point to different banks of memory, at different times. So,
a block needs to be identified not only by its starting address, but also by
which bank.

After tracing a block, the compiler will compile it down to a function. So it
emits machine code for the function prologue (align the stack, push registers,
etc.), then emits the machine code for each instruction.

Instructions that update registers, works like the one that I give as example
before, they just loads the registers from memory, do the operation, and stores
them back. A much more efficient way would be to keep the registers in the
machine registers, but I still haven't implemented that.

Instructions that may branch, checks if the target address is inside the block,
and if is, it emits a jump to the target address inside the block. This uses
[`dynasm`][dynasm] helper for emitting jumps. If the target isn't in the block,
it just emits the prologue for block (pop registers and call `ret`).

Instructions that read or write to memory emit function calls to Rust functions
that handle the memory access. This allows me to handle cases were it may change
the emulator state, like for the lazy update of components. But memory access
with immediate address may be emitted inline, if possible.

Another important point, is that writing to memory may trigger an interrupt, or
at least change its timing. This means that after each writing, the compiler
needs to emit another check for interrupts, and exit if needed. The same is true
for bank switching: a write to ROM could switch the bank that the block is
currently executing, so it needs to check if it did change and exit the block.

Also, because writes can trigger interrupts, and the interrupts can read the
content of flags, each write also compromises inter instructions optimizations,
like the omission of flag calculations that I mention before.

And because there are now multiple checks for interrupts throughout the block,
each check only needs to make sure that if the next interrupt will not happen
before the next interrupt check, including the initial check that happens before
entering the block.

After finishing emitting the block, the compiler moves the buffer with the
machine code to an executable memory region, and store it in a `HashMap`, that
maps `(bank, address)` to the block. Now, whenever the emulator needs to execute
this block again, it just gets it in the `HashMap` (or compile it if it isn't
there yet).

## Results

The entire point of implementing a JIT compiler is to improve the performance of
the emulator. So, how much of a speed-up did I get?

First, lets the compare the speed of the interpreter and the JIT compiler. For
that, I run the emulator with two different games, and measure the time that it
takes to run the game for 100 seconds. The first game is [Zelda: Link's
Awakening][zelda], and the second is [Tobu Tobu Girl][tobu]. Both games have a
nice start screen animation, which may be a good way to measure the speed of the
emulator.

[zelda]: https://en.wikipedia.org/wiki/The_Legend_of_Zelda:_Link%27s_Awakening
[tobu]: https://tangramgames.dk/tobutobugirl/

![Emulator comparison](/assets/gameroy_jit/interpreter-vs-jit.svg)

The emulator was capable of emulating Tobu Tobu Girl more than 100 times faster
than the real system! But comparing the two implementations, its is possible to
see that the JIT compiler decreases the execution time by about 40% in Tobu Tobu
Girl, and about 30% in Zelda. This is not the massive speed-up that I was
expecting, but is a very significant one.

We can also take a better look on the time spent in each component of the
emulator. Using [`blondie`][blondie] or [`inferno`][inferno] to capture a stack
trace, and passing it to my [plot script][plot], I can generate the following
pie charts:

[blondie]: https://github.com/nico-abram/blondie/
[inferno]: https://github.com/jonhoo/inferno
[plot]: https://github.com/Rodrigodd/gameroy/blob/4ac46fff3a203c20aadbb108585354701a52546e/tools/plot.py

![Interpreter time](/assets/gameroy_jit/per-component-inter-vs-jit.svg)

By the pie chart above, we can see that a big chunk of the time is spent in the
emulation of the PPU. We also can see that Zelda has a lot less opportunities for
optimizing the PPU, and therefore spends more time in it, which explains why it
is much slower and has a smaller speed up.

Looking just at the interpreter (blue) and JIT (red) sections, we can derive
(hand wavy) that the JIT compiler is about 4 times faster than the interpreter.
But considerable part of the time is still spent in just querying the block in
the `HashMap`. And even with the JIT compiler, some time is still spent in the
interpreter.

Another thing we can do is to compare our emulator with other ones out there. I
picked 4 emulators that are relatively popular and have a high degree of
accuracy: [SameBoy][sameboy], [BGB][bgb], [Beaten Dying Moon][bdm] and
[Emulicious][emulicious].

[sameboy]: https://sameboy.github.io/
[bgb]: http://bgb.bircd.org/
[bdm]: https://mattcurrie.com/bdm/
[emulicious]: https://emulicious.net/

To measure the performance of each emulator, I run the same two games as before,
and measure the time that it takes to run the intro animation of the game, using
a video recorded with [OBS Studio][obs]. For Tobu Tobu Girl, I let the intro run
three times, and for the Zelda, I let it run twice.

[obs]: https://obsproject.com/

The emulators were run on Windows, with they default GUI frontend, configure to
emulate DMG, and with "turbo mode" enabled. By the nature of using a GUI
frontend, the measure being a video recording, and the intro being run only two
or three times, the results are not that precise. But hopefully they are good
enough to compare them with each other.

![Emulator comparison](/assets/gameroy_jit/emulator-comparison.svg)

By these results, I can now claim that my emulator is the fastest one out there!
The interesting part is that even the interpreted version of my emulator is
faster than most of the other emulators. This is probably due to the
optimizations achieved by implementing the lazy update and the estimative of the
next interrupt.

Only Emulicious were faster than my interpret emulator when running Zelda (maybe
my emulator is invalidating the PPU optimization too much).

## Remaining work

I may have achieved the state-of-the-art in regard to dynamic recompilation
applied to Game Boy (the only related work that I know of is [JitBoy][jitboy]),
but my implementation is still far from perfect.

[jitboy]: https://github.com/sysprog21/jitboy

The first thing, is that I am not implementing some obvious optimizations. For
example, I am not keeping the Game Boy registers loaded in the `x64` registers,
but instead loading and saving them to memory every instruction. The only
register that I optimize is the PC register, whose value I can know at compile
time, so I only update it on block exit.

The flags are the most suboptimal, because I am computing the value of the flag,
encoding it in the F register bit flag, and them later decoding them in the
branch instructions, where I could being using the value of each flag directly
between instructions.

But I will probably not implement these optimizations, at least no directly. I
plan to instead write another backend for the compiler, using
[Cranelift][clift], that will very likely automatically handle these
optimizations and more. I have already [done some experiments with it
before][bf_clift], and it seems to work pretty well.

[bf_clift]: {% post_url 2022-11-21-bf_compiler-part3 %}

Another inefficiency in my JIT compiler, is in regard to redundant
recompilations. I am currently building an entire block for each `(bank, address)`
entry point, even if that same address is already in the middle of another
block. This means that many of the blocks are overlapping, and therefore
introducing unnecessary compilation time and memory usage.

I don't know if this is a big problem (I need to do some measuring first), but I
could fix it by including in the `HashMap` a key for the address of each
instruction in the block, and moving the block prelude to a Rust naked function.
But this would introduce some complications, like filling the `HashMap` with too
many keys, and the need to make sure that each compiled instruction can be
jumped to. This last point may be a problem for optimizations, but I didn't
think about it too much yet.

## Conclusion

And that is it! If you want to take a look at the emulator, I invite you to
check out [its GitHub repository][gameroy].

[gameroy]: https://github.com/Rodrigodd/gameroy
