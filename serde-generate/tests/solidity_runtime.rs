use crate::solidity_generation::get_bytecode;
use alloy_sol_types::sol;
use serde_generate::{solidity, CodeGeneratorConfig};
use std::{fmt::Display, fs::File, io::Write};
use serde_reflection::Samples;
use tempfile::tempdir;
use serde::{de::DeserializeOwned, {Deserialize, Serialize}};
use serde_reflection::{Tracer, TracerConfig};
use alloy_sol_types::SolCall as _;
use revm::db::InMemoryDB;
use revm::{
    primitives::{ExecutionResult, TxKind, Output, Bytes},
    Evm,
};


fn test_contract(bytecode: Bytes, encoded_args: Bytes) {
    let mut database = InMemoryDB::default();
    let contract_address = {
        let mut evm : Evm<'_, (), _> = Evm::builder()
            .with_ref_db(&mut database)
            .modify_tx_env(|tx| {
                tx.clear();
                tx.transact_to = TxKind::Create;
                tx.data = bytecode;
            })
            .build();

        let result : ExecutionResult = evm.transact_commit().unwrap();

        let ExecutionResult::Success { reason: _, gas_used: _, gas_refunded: _, logs: _, output } = result else {
            panic!("The TxKind::Create execution failed to be done");
        };
        let Output::Create(_, Some(contract_address)) = output else {
            panic!("Failure to create the contract");
        };
        contract_address
    };

    let mut evm : Evm<'_, (), _> = Evm::builder()
        .with_ref_db(&mut database)
        .modify_tx_env(|tx| {
            tx.transact_to = TxKind::Call(contract_address);
            tx.data = encoded_args;
        })
        .build();

    let result : ExecutionResult = evm.transact_commit().unwrap();

    let ExecutionResult::Success { reason: _, gas_used: _, gas_refunded: _, logs: _, output: _ } = result else {
        panic!("The TxKind::Call execution failed to be done");
    };

}


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TestVec<T> {
    pub vec: Vec<T>,
}




fn test_vector_serialization<T: Serialize + DeserializeOwned + Display>(t: TestVec<T>) -> anyhow::Result<()> {
    use crate::solidity_generation::print_file_content;
    // Indexing the types
    let mut tracer = Tracer::new(TracerConfig::default());
    let samples = Samples::new();
    tracer.trace_type::<TestVec<T>>(&samples).expect("a tracer entry");
    let registry = tracer.registry().expect("A registry");

    // The directories
    let dir = tempdir().unwrap();
    let path = dir.path();

    // The generated code
    let test_code_path = path.join("test_code.sol");
    {
        let mut test_code_file = File::create(&test_code_path)?;
        let name = "ExampleCodeBase".to_string();
        let config = CodeGeneratorConfig::new(name);
        let generator = solidity::CodeGenerator::new(&config);
        generator.output(&mut test_code_file, &registry).unwrap();

        let len = t.vec.len();
        let first_val = &t.vec[0];
        writeln!(
            test_code_file,
            r#"
contract ExampleCode is ExampleCodeBase {{

    constructor() {{
    }}

    function test_deserialization(bytes calldata input) external {{
      bytes memory input1 = input;
      TestVec memory t = bcs_deserialize_TestVec(input1);
      require(t.vec.length == {len}, "The length is incorrect");
      require(t.vec[0] == {first_val}, "incorrect value");

      bytes memory input_rev = bcs_serialize_TestVec(t);
      require(input1.length == input_rev.length);
      for (uint256 i=0; i<input1.length; i++) {{
        require(input1[i] == input_rev[i]);
      }}
    }}

}}

"#
        )?;

    }
    print_file_content(&test_code_path);


    // Compiling the code and reading it.
    let bytecode = get_bytecode(path, "test_code.sol", "ExampleCode")?;


    // Building the test entry
    let expected_input = bcs::to_bytes(&t).expect("Failed serialization");
    println!("expected_input={:?}", expected_input);
    println!("|expected_input|={}", expected_input.len());

    // Building the input to the smart contract
    sol! {
      function test_deserialization(bytes calldata input);
    }
    let input = Bytes::copy_from_slice(&expected_input);
    let fct_args = test_deserializationCall { input };
    let fct_args = fct_args.abi_encode();
    let fct_args = fct_args.into();


    test_contract(bytecode, fct_args);
    Ok(())
}



#[test]
fn test_vector_serialization_group() {
    let mut vec = vec![0 as u16; 3];
    vec[0] = 42;
    vec[1] = 5;
    vec[2] = 360;
    let t = TestVec { vec };
    test_vector_serialization(t).expect("successful run");

    let mut vec = vec![0 as u8; 2];
    vec[0] = 42;
    vec[1] = 5;
    let t = TestVec { vec };
    test_vector_serialization(t).expect("successful run");

    let mut vec = vec![0 as u32; 2];
    vec[0] = 42;
    vec[1] = 5;
    let t = TestVec { vec };
    test_vector_serialization(t).expect("successful run");

    let mut vec = vec![0 as i8; 2];
    vec[0] = -42;
    vec[1] = 76;
    let t = TestVec { vec };
    test_vector_serialization(t).expect("successful run");

    let mut vec = vec![0 as i16; 2];
    vec[0] = -4200;
    vec[1] = 7600;
    let t = TestVec { vec };
    test_vector_serialization(t).expect("successful run");

    let mut vec = vec![0 as i32; 2];
    vec[0] = -4200;
    vec[1] = 7600;
    let t = TestVec { vec };
    test_vector_serialization(t).expect("successful run");

    let mut vec = vec![0 as i64; 120];
    vec[0] = -4200;
    vec[1] = 7600;
    let t = TestVec { vec };
    test_vector_serialization(t).expect("successful run");
}



