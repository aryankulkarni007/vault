# roadmaps

LAYER 0: Transistor — COMPLETE

- (X) NMOS, PMOS
- (X) All CMOS gates
- (X) Iterative relaxation + Kahn's
- (X) Visualizer
- (~) Pending: clock driver, capacitor (when needed)

LAYER 1: Gate — CURRENT

- (X) Primitives: NOT, NAND, NOR, AND, OR, XOR, XNOR
- (~) Combinational units:
  - (X) half adder
  - (X) full adder
  - (X) 4-bit ripple carry adder
  - (X) MUX 2:1, MUX 4:1
  - ( ) decoder, encoder
- ( ) Sequential units (needs clock first):
  - ( ) SR latch
  - ( ) D flip flop
  - ( ) JK flip flop
  - ( ) D register (N-bit)
- ( ) GateSchematic — reconstruction metadata
- ( ) Visualizer — named blocks, same aesthetic
- ( ) Reconstruction engine v1 — gate → transistor drill down

LAYER 2: Functional — NEXT

- ( ) ALU
  - ( ) arithmetic: add, subtract
  - ( ) logical: AND, OR, XOR, NOT
  - ( ) comparison: equal, less than
  - ( ) shift: left, right
- ( ) Register file
  - ( ) Program counter
  - ( ) Instruction decoder
  - ( ) Memory interface (abstract)
  - ( ) Visualizer — functional blocks, data paths visible
  - ( ) Reconstruction engine v2 — functional → gate drill down

LAYER 3: CPU — THE MACHINE

- ( ) ISA definition — this is the fork point
  - ( ) instruction set designed here
  - ( ) every instr instruction maps to functional units
- ( ) Fetch/decode/execute pipeline
  - ( ) instruction fetch
  - ( ) decode
  - ( ) execute via ALU
  - ( ) writeback
- ( ) Runtime implementation alongside simulation
  - ( ) this is where bitwise replaces propagation
  - ( ) 1GHz this target starts here
- ( ) Visualizer — pipeline stages, register state, instruction stream
- ( ) Reconstruction engine v3 — instruction → functional → gate → transistor

LAYER 4: Memory — PARALLEL TO CPU

- ( ) SRAM — from cross coupled inverters
  - ( ) register file lives here
  - ( ) cache eventually
- ( ) DRAM — needs capacitor primitive
  - ( ) main memory
  - ( ) refresh controller
- ( ) Memory controller
  - ( ) address decoding
  - ( ) read/ addrewrite timing
- ( ) Bus architecture
  - ( ) needs tri-state buffers
  - ( ) address bus, data bus, control bus

LAYER 5: System — THE COMPUTER

- ( )CPU + Memory connected via bus
- ( ) Clock distribution
- ( ) Interrupt controller
- ( ) I/O abstraction
- ( ) This is a complete von Neumann machine

LAYER 6: Software — THE SOUL

- ( ) Assembler — text → machine code for your ISA
- ( ) Bootloader
- ( ) OS kernel
  - ( ) memory management
  - ( ) process scheduling
  - ( ) basic syscalls
- ( ) Shell
- ( ) User programs
