use crate::solidity_generation::{get_bytecode, get_registry_from_type};
use alloy_sol_types::sol;
use alloy_sol_types::SolCall as _;
use revm::db::InMemoryDB;
use revm::{
    primitives::{Bytes, ExecutionResult, Output, TxKind},
    Evm,
};
use serde::{
    de::DeserializeOwned,
    {Deserialize, Serialize},
};
use serde_generate::{solidity, CodeGeneratorConfig};
use std::{fmt::Display, fs::File, io::Write};
use tempfile::tempdir;

fn test_contract(bytecode: Bytes, encoded_args: Bytes) {
    let mut database = InMemoryDB::default();
    let contract_address = {
        let mut evm: Evm<'_, (), _> = Evm::builder()
            .with_ref_db(&mut database)
            .modify_tx_env(|tx| {
                tx.clear();
                tx.transact_to = TxKind::Create;
                tx.data = bytecode;
            })
            .build();

        let result: ExecutionResult = evm.transact_commit().unwrap();

        let ExecutionResult::Success { output, .. } = result else {
            panic!("The TxKind::Create execution failed");
        };
        let Output::Create(_, Some(contract_address)) = output else {
            panic!("Failure to create the contract");
        };
        contract_address
    };

    let mut evm: Evm<'_, (), _> = Evm::builder()
        .with_ref_db(&mut database)
        .modify_tx_env(|tx| {
            tx.transact_to = TxKind::Call(contract_address);
            tx.data = encoded_args;
        })
        .build();

    let result: ExecutionResult = evm.transact_commit().unwrap();

    let ExecutionResult::Success { .. } = result else {
        panic!("The TxKind::Call execution failed");
    };
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TestVec<T> {
    pub vec: Vec<T>,
}

fn test_vector_serialization<T: Serialize + DeserializeOwned + Display>(
    t: TestVec<T>,
) -> anyhow::Result<()> {
    let registry = get_registry_from_type::<TestVec<T>>();
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

    function test_deserialization(bytes calldata input) external {{
      TestVec memory t = bcs_deserialize_TestVec(input);
      require(t.vec.length == {len}, "The length is incorrect");
      require(t.vec[0] == {first_val}, "incorrect value");

      bytes memory input_rev = bcs_serialize_TestVec(t);
      require(input.length == input_rev.length);
      for (uint256 i=0; i<input.length; i++) {{
        require(input[i] == input_rev[i]);
      }}
    }}

}}
"#
        )?;
    }

    // Compiling the code and reading it.
    let bytecode = get_bytecode(path, "test_code.sol", "ExampleCode")?;

    // Building the test entry
    let expected_input = bcs::to_bytes(&t).unwrap();

    // Building the input to the smart contract
    sol! {
      function test_deserialization(bytes calldata input);
    }
    let input = Bytes::copy_from_slice(&expected_input);
    let fct_args = test_deserializationCall { input };
    let fct_args = fct_args.abi_encode().into();

    test_contract(bytecode, fct_args);
    Ok(())
}

#[test]
fn test_vector_serialization_types() {
    let mut vec = vec![0_u16; 3];
    vec[0] = 42;
    vec[1] = 5;
    vec[2] = 360;
    let t = TestVec { vec };
    test_vector_serialization(t).unwrap();

    let mut vec = vec![0_u8; 2];
    vec[0] = 42;
    vec[1] = 5;
    let t = TestVec { vec };
    test_vector_serialization(t).unwrap();

    let mut vec = vec![0_u32; 2];
    vec[0] = 42;
    vec[1] = 5;
    let t = TestVec { vec };
    test_vector_serialization(t).unwrap();

    let mut vec = vec![0_i8; 2];
    vec[0] = -42;
    vec[1] = 76;
    let t = TestVec { vec };
    test_vector_serialization(t).unwrap();

    let mut vec = vec![0_i16; 2];
    vec[0] = -4200;
    vec[1] = 7600;
    let t = TestVec { vec };
    test_vector_serialization(t).unwrap();

    let mut vec = vec![0_i32; 2];
    vec[0] = -4200;
    vec[1] = 7600;
    let t = TestVec { vec };
    test_vector_serialization(t).unwrap();

    let mut vec = vec![0_i64; 140];
    vec[0] = -4200;
    vec[1] = 7600;
    let t = TestVec { vec };
    test_vector_serialization(t).unwrap();
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SimpleEnumTestType {
    ChoiceA,
    ChoiceB,
    ChoiceC,
}

#[test]
fn test_simple_enum_serialization() -> anyhow::Result<()> {
    let registry = get_registry_from_type::<SimpleEnumTestType>();
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

        writeln!(
            test_code_file,
            r#"
contract ExampleCode is ExampleCodeBase {{

    function test_deserialization(bytes calldata input) external {{
      require(input.length == 1);
      SimpleEnumTestType t = bcs_deserialize_SimpleEnumTestType(input);
      require(t == SimpleEnumTestType.ChoiceB);

      bytes memory input_rev = bcs_serialize_SimpleEnumTestType(t);
      require(input_rev.length == 1);
      require(input[0] == input_rev[0]);
    }}

}}
"#
        )?;
    }

    // Compiling the code and reading it.
    let bytecode = get_bytecode(path, "test_code.sol", "ExampleCode")?;

    // Building the test entry
    let t = SimpleEnumTestType::ChoiceB;
    let expected_input = bcs::to_bytes(&t).unwrap();

    // Building the input to the smart contract
    sol! {
      function test_deserialization(bytes calldata input);
    }
    let input = Bytes::copy_from_slice(&expected_input);
    let fct_args = test_deserializationCall { input };
    let fct_args = fct_args.abi_encode().into();

    test_contract(bytecode, fct_args);
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StructBoolString {
    a: bool,
    b: String,
}

#[test]
fn test_struct_bool_string() -> anyhow::Result<()> {
    let registry = get_registry_from_type::<StructBoolString>();
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

        writeln!(
            test_code_file,
            r#"
contract ExampleCode is ExampleCodeBase {{

    function test_deserialization(bytes calldata input) external {{
      StructBoolString memory t = bcs_deserialize_StructBoolString(input);

      bytes memory input_rev = bcs_serialize_StructBoolString(t);
      require(input.length == input_rev.length);
      for (uint256 i=0; i<input.length; i++) {{
        require(input[i] == input_rev[i]);
      }}
    }}

}}
"#
        )?;
    }

    // Compiling the code and reading it.
    let bytecode = get_bytecode(path, "test_code.sol", "ExampleCode")?;

    // Building the test entry
    let t = StructBoolString {
        a: false,
        b: "abc".to_string(),
    };
    let expected_input = bcs::to_bytes(&t).unwrap();

    // Building the input to the smart contract
    sol! {
      function test_deserialization(bytes calldata input);
    }
    let input = Bytes::copy_from_slice(&expected_input);
    let fct_args = test_deserializationCall { input };
    let fct_args = fct_args.abi_encode().into();

    test_contract(bytecode, fct_args);
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ComplexEnumTestType {
    ChoiceA,
    Name(String),
    Age(i32),
}

#[test]
fn test_complex_enum_test() -> anyhow::Result<()> {
    let registry = get_registry_from_type::<ComplexEnumTestType>();
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

        writeln!(
            test_code_file,
            r#"
contract ExampleCode is ExampleCodeBase {{

    function test_deserialization(bytes calldata input) external {{
      ComplexEnumTestType memory t = bcs_deserialize_ComplexEnumTestType(input);

      bytes memory input_rev = bcs_serialize_ComplexEnumTestType(t);
      require(input.length == input_rev.length);
      for (uint256 i=0; i<input.length; i++) {{
        require(input[i] == input_rev[i]);
      }}
    }}

}}
"#
        )?;
    }

    // Compiling the code and reading it.
    let bytecode = get_bytecode(path, "test_code.sol", "ExampleCode")?;

    // Building the test entry
    let t1 = ComplexEnumTestType::ChoiceA;
    let t2 = ComplexEnumTestType::Name("joe".to_string());
    let t3 = ComplexEnumTestType::Age(43);
    for t in [t1, t2, t3] {
        let expected_input = bcs::to_bytes(&t).unwrap();

        // Building the input to the smart contract
        sol! {
            function test_deserialization(bytes calldata input);
        }
        let input = Bytes::copy_from_slice(&expected_input);
        let fct_args = test_deserializationCall { input };
        let fct_args = fct_args.abi_encode().into();

        test_contract(bytecode.clone(), fct_args);
    }
    Ok(())
}
