//! Two fixed pass-through shaders, authored here as SPIR-V 1.0 instructions.
//! This keeps even shader generation in Rust; there is no shader compiler at build time.

const INPUT: u32 = 1;
const OUTPUT: u32 = 3;
const LOCATION: u32 = 30;
const MAGIC: u32 = 0x0723_0203;

struct Module(Vec<u32>);
impl Module {
    fn new(id_bound: u32) -> Self {
        Self(vec![MAGIC, 0x0001_0000, 0, id_bound, 0])
    }
    fn op(&mut self, opcode: u16, args: &[u32]) {
        self.0
            .push(((args.len() as u32 + 1) << 16) | u32::from(opcode));
        self.0.extend_from_slice(args);
    }
    fn header(&mut self, model: u32, interfaces: &[u32]) {
        self.op(17, &[1]); // OpCapability Shader
        self.op(14, &[0, 1]); // Logical, GLSL450 memory model
        let mut entry = vec![model, 1, u32::from_le_bytes(*b"main"), 0];
        entry.extend_from_slice(interfaces);
        self.op(15, &entry);
    }
    fn common_types(&mut self) {
        self.op(19, &[2]); // id 2: void
        self.op(33, &[3, 2]); // id 3: void ()
        self.op(22, &[4, 32]); // id 4: f32
        self.op(23, &[5, 4, 4]); // id 5: vec4
        self.op(32, &[6, INPUT, 5]); // id 6: ptr input vec4
        self.op(32, &[7, OUTPUT, 5]); // id 7: ptr output vec4
    }
    fn finish(mut self) -> Vec<u32> {
        self.op(253, &[]); // OpReturn
        self.op(56, &[]); // OpFunctionEnd
        self.0
    }
}
pub(super) fn vertex() -> Vec<u32> {
    let mut m = Module::new(28);
    m.header(0, &[8, 9, 10, 13]);
    m.op(71, &[8, LOCATION, 0]);
    m.op(71, &[9, LOCATION, 1]);
    m.op(71, &[10, LOCATION, 0]);
    m.op(71, &[11, 2]); // per-vertex block
    m.op(72, &[11, 0, 11, 0]); // member 0: BuiltIn Position
    // Push-constant block containing one column-major 4x4 camera matrix.
    m.op(71, &[21, 2]);
    m.op(72, &[21, 0, 5]);
    m.op(72, &[21, 0, 7, 16]);
    m.op(72, &[21, 0, 35, 0]);
    m.common_types();
    m.op(24, &[20, 5, 4]);
    m.op(30, &[21, 20]);
    m.op(32, &[22, 9, 21]);
    m.op(32, &[23, 9, 20]);
    m.op(59, &[22, 24, 9]);
    m.op(30, &[11, 5]);
    m.op(32, &[12, OUTPUT, 11]);
    m.op(21, &[14, 32, 0]);
    m.op(43, &[14, 15, 0]); // u32(0)
    m.op(59, &[6, 8, INPUT]);
    m.op(59, &[6, 9, INPUT]);
    m.op(59, &[7, 10, OUTPUT]);
    m.op(59, &[12, 13, OUTPUT]);
    m.op(54, &[2, 1, 0, 3]);
    m.op(248, &[16]);
    m.op(61, &[5, 17, 8]);
    m.op(65, &[7, 18, 13, 15]);
    m.op(65, &[23, 25, 24, 15]);
    m.op(61, &[20, 26, 25]);
    m.op(145, &[5, 27, 26, 17]); // MatrixTimesVector
    m.op(62, &[18, 27]);
    m.op(61, &[5, 19, 9]);
    m.op(62, &[10, 19]);
    m.finish()
}
pub(super) fn fragment() -> Vec<u32> {
    let mut m = Module::new(12);
    m.header(4, &[8, 9]);
    m.op(16, &[1, 7]); // Fragment, OriginUpperLeft
    m.op(71, &[8, LOCATION, 0]);
    m.op(71, &[9, LOCATION, 0]);
    m.common_types();
    m.op(59, &[6, 8, INPUT]);
    m.op(59, &[7, 9, OUTPUT]);
    m.op(54, &[2, 1, 0, 3]);
    m.op(248, &[10]);
    m.op(61, &[5, 11, 8]);
    m.op(62, &[9, 11]);
    m.finish()
}
