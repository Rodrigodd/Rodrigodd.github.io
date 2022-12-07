---
layout: post
title:  "Compiling Brainfuck code - Part 1: An Optimized Interpreter"
date:   2022-10-21 15:00:00 -0300
modified_date:   2022-11-07 15:00:00 -0300
---

This is the first post of a blog post series where I will reproduce [Eli
Bendersky’s Adventures In JIT Compilation series][eli], but this time using the
[Rust programming language][rust]. I will also expand this series into
compiling native executables.

- **Part 1: An Optimized Interpreter**
- [Part 2: A Singlepass JIT Compiler]({% post_url 2022-10-16-bf_compiler-part2 %})
- [Part 3: A Cranelift JIT Compiler]({% post_url 2022-11-21-bf_compiler-part3 %})
- Part 4: A Static Compiler (WIP)

The goal of this series is to give a small intro into writing compilers, and
explore the tools in the Rust ecosystem that can be used for that.

Like in the original series, this will be achieved by writing a compiler for
the brainfuck programming language, in increasing degrees of complexity.

[eli]: https://eli.thegreenplace.net/2017/adventures-in-jit-compilation-part-1-an-interpreter
[rust]: https://www.rust-lang.org

# The Brainfuck language

[Brainfuck] is one of the most famous esoteric programming languages. It was
first designed by Urban Müller, with the goal of being a Turing complete language
whose compiler could be the smallest possible.

[Brainfuck]: https://esolangs.org/wiki/Brainfuck

By being esoteric, it isn't practical to write programs in but its simplicity
makes this language one of the easiest to implement, and will be used here to
lower the barrier in exploring more advanced topics in language implementation,
such as machine code generation.

The language only has 6 single character instructions. A program consists of a
sequence of these instructions, that act upon an array of memory cells, each
initialized with the value 0. There is also a pointer, initially pointing to
the first cell, that is used to interact with the memory.

The instructions are as follows:
- `>`: move the pointer to the right.
- `<`: move the pointer to the left.
- `+`: increase the value at the pointer by 1.
- `-`: decrease the value at the pointer by 1.
- `.`: output the value at the pointer to stdout.
- `,`: set the value at the pointer to the value read from stdin.
- `[`: jump forward to the matching `]` if the value at the pointer is 0.
- `]`: jump backward to the matching `[` if the value at the pointer is not 0.
- Any other symbol is treated as a comment and ignored.

For example, to print the numbers from 1 to 5, you can do:
```brainfuck
set cell 0 to 48 ('0' in ASCII)
++++++++++ ++++++++++ ++++++++++ ++++++++++ ++++++++
set cell 1 to 5
>+++++
increment cell 0; print it; decrement cell 1; loop while cell 1 is not 0
[<+.>-]
```

The language does not have an official specification, so some details may
vary from implementation to implementation, like the length of the array, or what the maximum
value of a memory cell is, and what happens on overflow.

I will follow [the original implementation][orig] here, where the array has a
length of 30,000 cells, each cell is 1 byte in size, and the value does a
two-complement wrapping on overflow. But my implementation will diverge on
stdin EOF behavior and the pointer will wrap around when it goes out of bounds.

[orig]: http://aminet.net/package/dev/lang/brainfuck-2

# Part 1: a basic interpreter

The easiest form of implementing a programming language is to write an
interpreter for it. For normal languages, this can be done, for example, by
reading the source code from a file, parsing it in an abstract syntax tree, and
walking through that tree, executing each of its instructions.

But for brainfuck, we do not need to do any actual parsing, and if we allow
brackets to not have a match, any input file will be valid brainfuck code.
This means that we can execute the brainfuck code directly from the source
file.

So in Rust, we can do the following:

```rust
fn main() -> std::io::Result<()> {
    let file_name = std::env::args().nth(1).unwrap();
    let source = std::fs::read(&file_name)?;

    let mut pointer = 0;
    let mut program_counter = 0;
    let mut memory = [0u8; 30_000];
    'program: loop {
        match source[program_counter] {
            b'+' => todo!(),
            b'-' => todo!(),
            b'.' => todo!(),
            b',' => todo!(),
            b'>' => todo!(),
            b'<' => todo!(),
            b'[' => todo!(),
            b']' => todo!(),
            _ => {} // do nothing, its only a comment
        }
        program_counter += 1;

        if source.len() == program_counter {
            break 'program;
        }
    }
    Ok(())
}
```

We read the file given by the first argument into a `Vec<u8>`, we define the
pointer, the program counter (the index of the current instruction) and the
memory, and in a loop, we execute an instruction and advance the program. When
the program counter reaches the end of the source, we terminate.

Now it only remains to implement the instructions. For `+` and `-` we increment
and decrement the value at the pointer, making sure to use wrapping:

```rust
b'+' => memory[pointer] = memory[pointer].wrapping_add(1),
b'-' => memory[pointer] = memory[pointer].wrapping_sub(1),
```

For `.` and `,` we print to stdout and read from stdin, respectively:

```rust
b'.' => print!("{}", memory[pointer] as char),
b',' => {
    use std::io::Read;
    std::io::stdin().read_exact(&mut memory[pointer..pointer + 1])?
}
```

For `>` and `<` we increment and decrement the pointer, making sure to wrap
around, and to not underflow the `usize`:

```rust
b'>' => pointer = (pointer + 1) % memory.len(),
b'<' => pointer = (pointer + memory.len() - 1) % memory.len(),
```

`[` and `]` are more complicated. If we pass the instruction condition, we
start traversing the program, updating `deep` for each `[` and `]` we find.
When the deep reaches 0, we have found the matching bracket. We also check if
the `program_counter` goes out of bounds (when there is no pair) and
terminate the program.

Note that at the end of the loop `program_counter` will be on the matching
bracket, which would cause an infinite loop, but this will be fixed by the
increment at the end of the program loop.

```rust
b'[' => {
    if memory[pointer] == 0 {
        let mut deep = 1;
        loop {
            if program_counter + 1 == source.len() {
                break 'program;
            }
            program_counter += 1;
            if source[program_counter] == b'[' {
                deep += 1;
            }
            if source[program_counter] == b']' {
                deep -= 1;
            }
            if deep == 0 {
                break;
            }
        }
    }
}
b']' => {
    if memory[pointer] != 0 {
        let mut deep = 1;
        loop {
            if program_counter == 0 {
                break 'program;
            }
            program_counter -= 1;
            if source[program_counter] == b']' {
                deep += 1;
            }
            if source[program_counter] == b'[' {
                deep -= 1;
            }
            if deep == 0 {
                break;
            }
        }
    }
}
```

And we are done! That code does not handle errors graciously, and unmatching
bracket simply terminates the program, but it's only [68 lines of
code][68lines]!

[68lines]: https://github.com/Rodrigodd/bf-compiler/blob/99cb9db00f03ee2c53bdc8e3b80e5da4c0976206/interpreter/src/main.rs

But of course we can do much better, and not only in error handling.
Brainfuck code could have a lot of comment characters, and we are traversing
all of them. This is particularly bad on loops.

To solve this, we can remove the comment symbols right after reading the file:

```rust
let source: Vec<u8> = source
    .into_iter()
    .filter(|b| [b'+', b'-', b'.', b',', b'>', b'<', b'[', b']'].contains(b))
    .collect();
```

But we can do even better. One of the principals of Rust is to make invalid
states unrepresentable. Here we have only 6 instructions, but it is being
represented by a `u8`, that contains 256 valid states. So, it makes more sense
to represent this instructions by an enum:

```rust
#[derive(PartialEq, Eq, Clone, Copy)]
enum Instruction {
    Increase,
    Decrease,
    MoveRight,
    MoveLeft,
    Input,
    Output,
    JumpRight,
    JumpLeft,
}
```

Then the previous code becomes:

```rust
let instructions: Vec<Instruction> = source
    .into_iter()
    .filter_map(|b| match b {
        b'+' => Some(Instruction::Increase),
        b'-' => Some(Instruction::Decrease),
        b'.' => Some(Instruction::Output),
        b',' => Some(Instruction::Input),
        b'>' => Some(Instruction::MoveRight),
        b'<' => Some(Instruction::MoveLeft),
        b'[' => Some(Instruction::JumpRight),
        b']' => Some(Instruction::JumpLeft),
        _ => None,
    })
    .collect();
```

Also, instead of keeping the variables of the program state in the `main`, we
can encapsulate them in a struct.

```rust
struct Program {
    program_counter: usize,
    pointer: usize,
    instructions: Vec<Instruction>,
    memory: [u8; 30_000]
}
```

Then we add a method that takes care of transforming the source code in a
program:

```rust
impl Program {
    fn new(source: &[u8]) -> Program {
        let instructions: Vec<Instruction> = source
            .iter()
            .filter_map(|b| match b {
                // ...
            })
            .collect();

        Program {
            program_counter: 0,
            pointer: 0,
            instructions,
            memory: [0; _],
        }
    }
}
```

A method for running the Program:

```rust
impl Program {
    // ...

    fn run(&mut self) -> std::io::Result<()> {
        'program: loop {
            use Instruction::*;
            match self.instructions[self.program_counter] {
                // ...
            }
            self.program_counter += 1;

            if self.instructions.len() == self.program_counter {
                break 'program;
            }
        }
    }
}
```

And the main becomes only:

```rust
fn main() -> std::io::Result<()> {
    let file_name = std::env::args().nth(1).unwrap();
    let source = std::fs::read(&file_name)?;

    Program::new(&source).run()
}
```

Then we can [lock and flush stdout on output][stdout], [handle stdin
EOF][stdin], [fix Windows issues][windows], [handle errors in a user-friendly
manner][friendly] and [fix another Windows issue][windows2]. And done! We have
an idiomatic and well-written interpreter in [157 lines of Rust code][157lines].

[stdout]: https://github.com/Rodrigodd/bf-compiler/commit/2d8414c344192fee756e6a9ec70e50e886dac2bc
[stdin]: https://github.com/Rodrigodd/bf-compiler/commit/b0bf662ff457bbb74e2e4e677daab54fe70c3f15
[windows]: https://github.com/Rodrigodd/bf-compiler/commit/4128ca06e0db8e2caa60a3925af73348573a58bc
[friendly]: https://github.com/Rodrigodd/bf-compiler/commit/ffc7a64a1f5b33f7ced49abda52de8da9f97532f
[windows2]: https://github.com/Rodrigodd/bf-compiler/commit/396cabe0a9db3d453bb8e6203ae805cde7a3d1df
[157lines]: https://github.com/Rodrigodd/bf-compiler/blob/396cabe0a9db3d453bb8e6203ae805cde7a3d1df/interpreter/src/main.rs

## Measuring the interpreter performance

The goal here is to progressively implement more complex versions of the
interpreter/compiler, and that complexity aims to increase the runtime
performance of our brainfuck programs. So it is important that we keep track of
how fast our interpreter is getting.

Here we would like to have a suite of programs to run that could represent the
real word distribution of brainfuck programs. But because doing so is far from
trivial, I will follow the original blog post here, and measure the execution
time when running [a Mandelbrot generator][mandel], by Erik Bosma, and [a
factorization program][factor], by Brian Raiter, with the input 179424691.

[mandel]: https://github.com/Rodrigodd/bf-compiler/blob/490179516e8c8e2f2d26f31c7bafd40e1faf65fa/programs/mandelbrot.bf
[factor]: https://github.com/Rodrigodd/bf-compiler/blob/490179516e8c8e2f2d26f31c7bafd40e1faf65fa/programs/factor.bf

The current interpreter has run mandelbrot.bf in 96.1 seconds and factor.bf in 27.68 seconds[^bench][^elis_bench].

[^bench]: You can find all measures in detail, including method and
    environment, [in this document][benchmark.adoc].

[^elis_bench]: The implementation in the original blog post have taken, respectively, 64
    seconds and 24 seconds on my machine. But my implementation is doing extra
    things like memory wrap-around and (unnecessary) bound checks, which may be
    causing this big difference.

[benchmark.adoc]: https://github.com/Rodrigodd/bf-compiler/blob/master/benchmark.adoc

## Optimizing our interpreter

We can optimize our interpreter in multiple ways. That could be done by making
some micro optimization to our interpreter code, like replacing indexing by raw
pointers, looking at generated assembly for optimization opportunities, etc.
But before that, we have even greater optimization opportunities.

### Precomputing the paring brackets addresses.

Every time we encounter a `[` or `]`, and pass the jump condition, we search
for the paring bracket. But the pairing brackets are always the same, so there is
no need to search them every time.

So the most obvious optimization that we can do now is to precompute the pairing
branch of each bracket, and store them somewhere, and every time we execute a
jump instruction, we look up the address of the pairing bracket.

In our case we will store the pairing bracket as a field of our `Instruction`
enum, because it is the simpler thing to do, and later more instructions will
have a field.

So our `Instruction` enum becomes[^instruction_size]:

[^instruction_size]: Note that here I am using a `usize` for the index of the
    pairing bracket, which has 8 bytes on a 64-bit system. This is causing the
    `Instruction` enum to have a size of 16 bytes (due to the discriminant and
    alignment). So a possible optimization could be to use a `u32` or a 
    `u16` here, depending on how big brainfuck programs your interpreter
    needs to support.


```rust
#[derive(PartialEq, Eq, Clone, Copy)]
enum Instruction {
    Increase,
    Decrease,
    MoveRight,
    MoveLeft,
    Input,
    Output,
    JumpRight(usize),
    JumpLeft(usize),
}
```

And the instructions' execution will become much simpler:

```rust
use Instruction::*;
match self.instructions[self.program_counter] {
    // ...
    JumpRight(pair_address) => {
        if self.memory[self.pointer] == 0 {
            self.program_counter = pair_address;
        }
    }
    JumpLeft(pair_address) => {
        if self.memory[self.pointer] != 0 {
            self.program_counter = pair_address;
        }
    }
}
self.program_counter += 1;
```

Now, for computing the bracket pair we can use a stack: every time we encounter
a `[` we push its address to the stack (filling `JumpRight` with a dummy
address), and every time we encounter a `]` we pop from the stack, and fix both
jump instructions' addresses.

So in `Program::new` we replace the `filter_map` by:

```rust
let mut instructions = Vec::new();
let mut bracket_stack = Vec::new();

for b in source {
    let instr = match b {
        // ...
        b'[' => {
            let curr_address = instructions.len();
            bracket_stack.push(curr_address);
            // will be fixed at the pair ']'.
            Instruction::JumpRight(0)
        }
        b']' => {
            let curr_address = instructions.len();
            match bracket_stack.pop() {
                Some(pair_address) => {
                    instructions[pair_address] = Instruction::JumpRight(curr_address);
                    Instruction::JumpLeft(pair_address)
                }
                None => return Err(UnbalancedBrackets(']', curr_address)),
            }
        }
        _ => continue,
    };
    instructions.push(instr);
}
```

Also notice that there could be more `]` than `[`, making the pop return
`None`. In that case we are returning an error. Because this is the only
possible error when parsing brainfuck code, I am defining the error as a simple
tuple struct instead of a `enum`, as it is usually done:

```rust
struct UnbalancedBrackets(char, usize);
```

And the signature of `Program::new` becomes:

```rust
impl Program {
    fn new(source: &[u8]) -> Result<Program, UnbalancedBrackets> {
        // ...
        Ok(Program {
            program_counter: 0,
            pointer: 0,
            instructions,
            memory: [0; 30_000],
        })

    }
}
```

Ah, and we also cannot forget that there could also be more `[` than `]`:

```rust
let mut instructions = Vec::new();
let mut bracket_stack = Vec::new();

for b in source {
    // ...
    instructions.push(instr);
}

if let Some(unpaired_bracket) = bracket_stack.pop() {
    return Err(UnbalancedBrackets('[', unpaired_bracket));
}
```

And finally we need to handle that error in main:

```rust
fn main() -> ExitCode {
    let mut args = std::env::args();
    if args.len() != 2 {
        eprintln!("expected a single file path as argument");
        return ExitCode::from(1);
    }

    let file_name = args.nth(1).unwrap();
    let source = match std::fs::read(&file_name) {
        Ok(x) => x,
        Err(err) => {
            eprintln!("Error reading '{}': {}", file_name, err);
            return ExitCode::from(2);
        }
    };

    let mut program = match Program::new(&source) {
        Ok(x) => x,
        Err(UnbalancedBrackets(c, address)) => {
            eprintln!(
                "Error parsing file: didn't find pair for `{}` at instruction index {}",
                c, address
            );
            return ExitCode::from(3);
        }
    };

    if let Err(err) = program.run() {
        eprintln!("IO error: {}", err);
    }

    ExitCode::from(0)
}
```

It would be more useful to return the line and column of the bracket (at least
the byte index would be better), but this is good enough for our needs.

Running the programs again:

![](/assets/brainfuck/plot0.svg "A mean decrease of 26.63% from the previous result"){:style="display:block; margin-left:auto; margin-right:auto"}

A decrease of 25%! But we can do better.

### Replace repeating operations by a single one

If you take a look at the source code of [mandelbrot.bf][mandel], or of
[factor.bf][factor], or of the example hello-word at the start of this page,
you will see that there are a lot of instructions repeating in sequence.

Let start with increment. So instead of repeating an increment `n` times, we can
replace them by a single add `n` instruction. Even better, we can replace both
increment and decrement instructions by a single `Add(u8)` instruction,
because a subtraction by `n` can be represented as an addition via the
two-complement on `n` (or `n.wrapping_neg()` in our case).

So `Instruction` becomes:

```rust
#[derive(PartialEq, Eq, Clone, Copy)]
enum Instruction {
    Add(u8),
    MoveRight,
    MoveLeft,
    Input,
    Output,
    JumpRight(usize),
    JumpLeft(usize),
}
```

The instruction interpreting will become:

```rust
use Instruction::*;
match self.instructions[self.program_counter] {
    Add(n) => self.memory[self.pointer] = self.memory[self.pointer].wrapping_add(n),
    // ...
}
self.program_counter += 1;
```

And the parsing:

```rust
for b in source {
    let instr = match b {
        b'+' | b'-' => {
            let inc = if *b == b'+' { 1 } else { 1u8.wrapping_neg() };
            if let Some(Instruction::Add(value)) = instructions.last_mut() {
                *value = value.wrapping_add(inc);
                continue;
            }
            Instruction::Add(inc)
        }
        // ...
    };
    instructions.push(instr);
}
```
Running it:

![](/assets/brainfuck/plot1.svg "A mean decrease of 0.26% from the previous result"){:style="display:block; margin-left:auto; margin-right:auto"}

Well... it was not as big of a reduction as I was expecting, only 0.26%. 0.26 ± 0.37
percent to be more precise. It is even uncertain that it really decreased.

But let's do the same for the `Move` instructions:

```rust
#[derive(PartialEq, Eq, Clone, Copy)]
enum Instruction {
    Add(u8),
    Move(isize)
    Input,
    Output,
    JumpRight(usize),
    JumpLeft(usize),
}
```

```rust
use Instruction::*;
match self.instructions[self.program_counter] {
    // ...
    Move(n) => {
        let len = self.memory.len() as isize;
        let n = (len + n % len) as usize;
        self.pointer = (self.pointer + n) % len as usize;
    }
    // ...
}
self.program_counter += 1;
```

```rust
for b in source {
    let instr = match b {
        // ...
        b'>' | b'<' => {
            let inc = if *b == b'>' { 1 } else { -1 };
            if let Some(Instruction::Move(value)) = instructions.last_mut() {
                *value += inc;
                continue;
            }
            Instruction::Move(inc)
        }
        // ...
    };
    instructions.push(instr);
}
```

Running again:

![](/assets/brainfuck/plot2.svg "A mean decrease of 71.3% from the previous result"){:style="display:block; margin-left:auto; margin-right:auto"}

Wow! This is what I was looking for, more than 3 times in performance increase!

But now, there is the question: Why does optimizing the increment/decrement result
in such a small improvement, but optimizing the move instructions causes
such a big change? Well, if I had paid more attention to the source code of
[mandelbrot.bf][mandel] and of [factor.bf][factor], I could have noticed that
there are much more '>'s and '<'s than '+'s and '-'s:

|   | mandelbrot.bf | factor.bf |
|:-:|--------------:|----------:|
| + | 694           | 422       |
| - | 591           | 370       |
| > | 4,428         | 1,324     |
| < | 4,363         | 1,294     |
| [ | 686           | 231       |
| ] | 686           | 231       |
| . | 3             | 7         |
| , | 0             | 1         |

But what's really important is the number of times these instructions are
executed at runtime. In that case we can [modify our interpreter][prof] to
count the number of instructions being executed:

[prof]: https://github.com/Rodrigodd/bf-compiler/commit/17a8fce11bc23cd776d6b9913da40b4684fd7399

|       | mandelbrot.bf  | factor.bf     | mandelbrot.bf | factor.bf     |
|:----- | --------------:| -------------:| -------------:| -------------:|
| +     | 179,053,599    | 212,428,900   | 351,460,869   | 396,481,127   |
| \-    | 177,623,022    | 212,328,376   | 0             | 0             |
| \>    | 4,453,036,023  | 1,220,387,724 | 1,408,648,727 | 484,719,480   |
| <     | 4,453,036,013  | 1,220,387,704 | 0             | 0             |
| \[    | 422,534,152    | 118,341,127   | 422,534,152   | 118,341,127   |
| \]    | 835,818,921    | 242,695,606   | 835,818,921   | 242,695,606   |
| ,     | 6,240          | 21            | 6,240         | 21            |
| ,     | 0              | 10            | 0             | 10            |
| ----- | -------------- | ------------- | ------------- | ------------- |
| Total | 10,521,107,970 | 3,226,569,468 | 3,018,468,909 | 1,242,237,371 |

Now, with this data, everything becomes much clearer. The `Add` optimization
has decreased the total number of operation by only 0.24%, and the `Move`
optimization has decreased it by 69%!

But, of course, we can always do better.

### Optimizing higher constructs

So we have already implemented an optimization that changes a single instruction,
and one that replaces a repeated instruction by a single one. The next step is
to replace a set of distinct instructions by a single one.

In brainfuck, there are a lot of basic operations that need to involve multiple
instructions. For example, clearing a cell (`[-]` or `[+]`), adding a value of
one cell to another (add to the third right would be `[->>>+<<<]`, but this
clears the original value), duplicating a cell (`[->>>+>+<<<<]`), etc.

Many of these operations are basically loops with instructions inside. In this
case, we can profile our programs by counting how many times a given loop is
run, which boils down to counting how many times each particular `]` is
executed. And then we present this data by deduplicating identical loops and
displaying them in descending order.

Running [the new profiling][prof_loop] for `factor.bf`:

[prof_loop]: https://github.com/Rodrigodd/bf-compiler/commit/17b1744ee535b3977cb3306527afd16b99b59804

|   | count     | loop
|:-:|----------:|:-----------
| 1 | 32,276,219| [-1<10+1>10]
| 2 | 28,538,377| [-1]
| 3 | 15,701,515| [-1<4+1>4]
| 4 | 12,581,941| [-1>3+1>1+1<4]
| 5 |  9,579,970| [-1>3+1>2+1<5]
| 6 |  9,004,028| [-1<3+1>3]
| 7 |  6,093,976| [-1<1-1>1]
| 8 |  6,085,735| [-1>3+1<3]
| 9 |  5,853,530| [-1<1+1<3+1>4]
|10 |  5,586,229| [-1>3+2<3]

In first place comes the loop `[-<<<<<<<<<<+>>>>>>>>>]` (the annotation above
is in compressed format), the "add cell to another" operation, running 32
million of times! In fact, positions 3, 6, 7, 8 and 10 are also the same
operation. Position 2 is the "clear cell operation" (`[-]`), and position 4, 5
and 9 is the "duplicate cell" (`[->>>+>+<<<<]`).

For the `mandelbrot.bf`:

|   | count        | loop
|:-:|-------------:|:------------------
| 1 |  287,432,488 | [>9]
| 2 |  200,272,618 | [<9]
| 3 |  116,145,344 | [>1[-1>9+1<9]<10]
| 4 |   32,021,044 | [-1>9+1<9]
| 5 |   31,339,760 | [>2[-1>9+1<9]<11]
| 6 |   12,637,333 | [-1>2[-1<2+1>2]<2[-1>2+1>2+1<4]+1>9]
| 7 |   12,038,491 | [-1]
| 8 |   11,813,904 | [-1>2[-1<2+1>2]<2[-1>2+1>1+1<3]+1>9]
| 9 |    9,515,168 | [>1+1>8]
|10 |    9,017,333 | [-1<4+1>1[<1-1>1-1<6+1>6]<1[-1>1+1<1]>4]

The mandelbrot one was less predictable. In the first and second place there is
the "move 9 cell right/left repeatedly until it encounters a 0". In 4, it is the
"add cell to another". 7 is the "clear cell" operation.

The other loops appear to be doing more specific operations (which may be giving
a glimpse on how the mandelbrot algorithm works). 3, which is the outer loop of 4,
appears to be shifting the elements of an array, where each element contains 9
cells. 6 and 8 appear to be duplicating some cell for each element of the
array, using a temporary cell for that. 10 is more complicated but appears to
be another operation in the array.

Well, each of the operations described above can be executed by the computer in
a much more efficient way if they are implemented by a "real" programming
language. So, we can identify some of these operations in the source code, and
replace them by a single instruction implemented in Rust.

In this case, I will follow the original blog posts, and implement instructions
for the "clear cell", "add cell to another" and "move until 0" operators.

Let's start with the "clear cell":

```rust
#[derive(PartialEq, Eq, Clone, Copy)]
enum Instruction {
    // ..
    Clear,
}
```

Its implementation is simple enough:

```rust
'program: loop {
    use Instruction::*;
    match self.instructions[self.program_counter] {
        // ...
        Clear => self.memory[self.pointer] = 0,
    }
    // ...
}
```

And every time we parse a `]` we check if the previous instructions match the
"clear cell" operation, and replace it by the new instruction.

```rust
b']' => {
    let curr_address = instructions.len();
    match bracket_stack.pop() {
        Some(pair_address) => {
            instructions[pair_address] = Instruction::JumpRight(curr_address);

            use Instruction::*;
            match instructions.as_slice() {
                // could enter an infinite loop if n is even.
                [.., JumpRight(_), Add(n)] if n % 2 == 1 => {
                        let len = instructions.len();
                        instructions.drain(len - 2..);
                        Instruction::Clear
                    }
                _ => Instruction::JumpLeft(pair_address),
            }
        }
        None => return Err(UnbalancedBrackets(']', curr_address)),
    }
}
```

Running it:

![](/assets/brainfuck/plot3.svg "A mean decreased of 4.5% from the previous result"){:style="display:block; margin-left:auto; margin-right:auto"}

A mean decrease of 4.5% in execution time. And it decreased the total number of
instructions by 0.80% and 4.59%, for `mandelbrot.bf` and `factor.bf`,
respectively.

Now for the "add to another":

```rust
#[derive(PartialEq, Eq, Clone, Copy)]
enum Instruction {
    // ..
    AddTo(isize),
}
```

```rust
'program: loop {
    use Instruction::*;
    match self.instructions[self.program_counter] {
        // ...
        AddTo(n) => {
            let len = self.memory.len() as isize;
            let n = (len + n % len) as usize;
            let to = (self.pointer + n) % len as usize;

            self.memory[to] = self.memory[to].wrapping_add(self.memory[self.pointer]);
            self.memory[self.pointer] = 0
        }
    }
    // ...
}
```

```rust
match instructions.as_slice() {
    // ...
    &[.., JumpRight(_), Add(255), Move(x), Add(1), Move(y)]
        if x == -y =>
    {
        let len = instructions.len();
        instructions.drain(len - 5..);
        Instruction::AddTo(x)
    }
    _ => Instruction::JumpLeft(pair_address),
}
```

![](/assets/brainfuck/plot4.svg "A mean decrease of 15.3% from the previous result"){:style="display:block; margin-left:auto; margin-right:auto"}

This one gives more substantial performance increase. A mean decrease in
run-time by 15.3%. Only 2.56% on mandelbrot, but 28.13% on factor!

And finally the "move until 0" operator:


```rust
#[derive(PartialEq, Eq, Clone, Copy)]
enum Instruction {
    // ..
    MoveUntil(isize),
}
```

```rust
'program: loop {
    use Instruction::*;
    match self.instructions[self.program_counter] {
        // ...
        MoveUntil(n) => {
            let len = self.memory.len() as isize;
            let n = (len + n % len) as usize;
            loop {
                if self.memory[self.pointer] == 0 {
                    break;
                }

                self.pointer = (self.pointer + n) % len as usize;
            }
        }
    }
    // ...
}
```

```rust
match instructions.as_slice() {
    // ...
    &[.., JumpRight(_), Move(n)] => {
        let len = instructions.len();
        instructions.drain(len - 2..);
        Instruction::MoveUntil(n)
    }
    _ => Instruction::JumpLeft(pair_address),
}
```

![](/assets/brainfuck/plot5.svg "A mean decrease of 11.2% from the previous result"){:style="display:block; margin-left:auto; margin-right:auto"}

As expected from the previous profiles, this optimization has greater impact on
the mandelbrot.bf. Time goes down by 18.61% on mandelbrot and 3.79% on factor!

# About Interpreters and Compilers

What we basically have done through this post, is to transform the
program source code into instructions that can be interpreted in a more
efficient way. We also looked for common patterns in the code and created
specific instructions for them, increasing the performance even more.

These are already basic ideas of what interpreters and compilers do.
Instead of interpreting/compiling the code directly from its source code (like
how AST-walking interpreters, or single pass compilers do), they most of the
time convert them to an intermediate representation (IR) that is more suitable
for interpreting, compiling, applying optimizations, etc.

For example, in Lua an AST is not even created, the source code is directly
compiled, in a single pass, to *bytecode*, which is the IR used by the Lua
interpreter.

A more complex example would be Rust. In Rust, the code AST is first converted
to a High-Level Intermediate Representation (process called HIR lowering),
which is used for type inference, trait solving and type checking. After that,
it is lowered to Mid-Level Intermediate Representation (MIR), which is used for
borrow checking, applying optimizations and monomorphization collecting. And
finally, MIR is converted to LLVM-IR, which is passed to LLVM to apply more
optimizations and only then compiled down to machine code. [More details can be found
here][rust_overview].

[rust_overview]: https://rustc-dev-guide.rust-lang.org/overview.html

# Future work

![](/assets/brainfuck/plot6.svg "A decrease of 8.9% from the previous result"){:style="display:block; margin-left:auto; margin-right:auto"}

From the first implementation to the last one we get a speedup of almost 7x.
Of course, we can always do better. We could continue implementing specific
instructions for common brainfuck operations, and we could do that on and on,
without end (we could even make an instruction that prints the factors of a
number!), but they would less and less generalize for all programs.

We could also start searching for micro optimizations, like inspecting the
generated assembly for lost optimization opportunities, and modify the code to
reach them, etc. We can also go `unsafe` and remove bound checks with
`get_unchecked` and `unreachable_unchecked`[^did_that].

[^did_that]: In fact, [I have done some of these changes here][micro], but
    didn't carefully profile it.

[micro]: https://github.com/Rodrigodd/bf-compiler/commit/b6591ebd367d89cceff166df09ca500c172d3873

But this being a normal interpreter, there will always be a significant
overhead introduced by the interpreter loop that cannot be removed (unless we
manage to transform the source code to a single instruction).

So we will stop enhancing this interpreter for now, and, in the next part, we
will build a JIT interpreter, that will convert our source code directly to
machine code, and remove the interpreter loop altogether.

