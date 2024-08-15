use core::panic;
use std::{collections::VecDeque, str::from_utf8};
use serde_json::{self, Value};
use bincode;


pub struct Parser{
    order: Vec<Value>,
    configs: Value
}

impl Parser{
    pub fn new(scheme: Value)->Parser{//need to come up with a way to feed in a string json
        let order = scheme.get("required").expect("Could not find 'required' property, is the scheme correct?").as_array().expect("Required property must be an array").clone();
        let configs = scheme.get("properties").unwrap().clone();
        Parser {order,configs}  
    }
    pub fn new_from_string(scheme:String)->Parser{
        let json:Value = serde_json::from_str(&scheme).expect("String is not valid JSON");
        Self::new(json)
    }
    pub fn encode_from_string(&self,message:&String)->Vec<u8>{
        let data:serde_json::Value = serde_json::from_str(message).expect("Could not deserialize message, is it valid JSON?");//Will be validated upstream, temp warning
        self.encode(data)
    }
    pub fn encode(&self,message:Value)->Vec<u8>{//Should this be a string or a value? I don't think I want to expose serde_json, but I am not sure
        //Maybe should be encode_from_str and encode
        //Can assume this is correctly packed
        let mut processed_data = vec![];
        for i in &self.order{
            let unprocessed_data = message.get(i.as_str().unwrap()).unwrap();
            let current_config = self.configs.get(i.as_str().unwrap()).unwrap();
            let output:Vec<u8>;
            match current_config.get("enum"){
                Some(x) => {
                    output = Self::to_bytes(current_config.get("enum_encoding").unwrap().get(x.as_array().unwrap().into_iter().position(|x| x.as_str()== unprocessed_data.as_str()).unwrap()).unwrap(),1)
                    //Do I need encoding? Or can I just trust it remains in order?
                },
                None => {//not enum
                    match current_config.get("type").unwrap().as_str().unwrap(){
                        "boolean" => {
                            output = Self::to_bytes(unprocessed_data, 1);
                        },
                        "integer" => {
                            let len = current_config.get("maximum").expect("Number fields must have a declared maximum").as_u64().expect("Maximum Must be a number");
                            if len%256 != 0{
                                panic!("Maximum must be a multiple of 8");
                            } else if len < 256{
                                panic!("Length must be at least one byte (atm)")
                            }
                            //assume its always signed for now
                            output = Self::to_bytes(unprocessed_data, len as usize/256);
                        },
                        "string" => output = Self::to_bytes(unprocessed_data, 0),//0 means don't handle length and remain little_endian so the length is first
                        "number" => output = Self::to_bytes(unprocessed_data,8),//handle signed bits here too!
                        _ => panic!("Cannot parse")
                    }
                },
            }
            processed_data.push(output)
        }
    processed_data.into_iter().flatten().collect()//still need to add pre-append bits
    }
    pub fn decode_to_string(&self,message:Vec<u8>)->String{
        let data = self.decode(message);
        return serde_json::to_string(&data).unwrap();
    }
    pub fn decode(&self,message: Vec<u8>,)->Value{//still need to handle preappend bits
        let mut working_message:VecDeque<u8> = message.into();
        let mut output = serde_json::Map::new();
        for i in &self.order{
            let current_config = self.configs.get(i.as_str().unwrap()).unwrap();
            match current_config.get("enum"){
                Some(x) => {
                    let data:u8 = working_message.pop_front().unwrap();
                    output.insert(i.as_str().unwrap().to_string(),x.as_array().unwrap().get(current_config.get("enum_encoding").unwrap().as_array().unwrap().into_iter().position(|x| x.as_u64().unwrap() as u8 == data).unwrap()).unwrap().clone());
                },
                None => {
                    match current_config.get("type").unwrap().as_str().unwrap(){
                        "boolean" => {
                            let data:u8 = working_message.pop_front().unwrap();
                            if data == 1{
                                output.insert(i.as_str().unwrap().to_string(),Value::Bool(true));
                            } else {
                                output.insert(i.as_str().unwrap().to_string(),Value::Bool(false));
                            }
                        },
                        "integer" => {
                            let len = current_config.get("maximum").expect("Number fields must have a declared maximum").as_u64().expect("Maximum Must be a number");
                            if len%256 != 0{
                                panic!("Maximum must be a multiple of 8");
                            } else if len < 256{
                                panic!("Length must be at least one byte (atm)")
                            }
                            let mut data:Vec<u8> = working_message.drain(0..len as usize/256).collect();
                            data.reverse();
                            while data.len() <8{
                                data.push(0)
                            }
                            let working_output:u64 = bincode::deserialize(&data).unwrap();
                            output.insert(i.as_str().unwrap().to_string(),Value::Number(working_output.into()));
                        },
                        "number" => {
                            let data:Vec<u8> = working_message.drain(0..8).collect();
                            let working_output:u64 = bincode::deserialize(&data).unwrap();
                            output.insert(i.as_str().unwrap().to_string(),Value::Number(working_output.into()));
                        },
                        "string" => {
                            let length  = working_message.pop_front().unwrap();
                            let data:Vec<u8> = working_message.drain(0..length as usize).collect();
                            let working_output:String = from_utf8(&data).expect("Can't convert to UTF8").to_string();
                            output.insert(i.as_str().unwrap().to_string(),Value::String(working_output)); 
                        },
                        _=> panic!("Not implemented for decoding")
                    }
                },
            }
        }
        Value::from(output) //Need to convert back into JSON
    }
    fn to_bytes(preprocessed_data: &serde_json::Value,len:usize) -> Vec<u8> {
        match preprocessed_data{
            serde_json::Value::Null => panic!("Cannot have a null field value"), //Implies bad frame decode
            serde_json::Value::Object(_) => todo!(),
            _=>{}
        }
        let starting = bincode::serialize(preprocessed_data).expect("Couldn't convert to bytes");
        if len ==0{
            let length = starting[0];
            let mut carry = starting;
            carry.reverse();
            let mut output:Vec<u8> = carry.into_iter().take(length as usize).collect();
            output.push(length);
            output.reverse();
            return output
        } else {
            let mut carry:Vec<u8> = starting.into_iter().take(len).collect();
            carry.reverse();
            return carry
        }
    }
}


#[cfg(test)]
mod tests{
    use std::fs;

    use super::*;
    #[test]
    fn test_loading(){
        Parser::new_from_string(fs::read_to_string(r"src\test_files\scheme.json").expect("Could not read schema file"));
        assert!(true)
    }
    #[test]
    fn test_encoding(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\scheme.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src\test_files\Incoming_data.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message);
        let expected_message = [1, 50, 4, 84, 101, 115, 116, 1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_decoding(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\scheme.json").expect("Could not read schema"));
        let message = [1, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message = parser.decode(message.to_vec());
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src\test_files\Incoming_data.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message,expected_message)
    }
    #[test]
    fn test_encode_then_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\scheme.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src\test_files\Incoming_data.json").expect("Could not read incoming data file");
        let encoded = parser.encode_from_string(&message);
        let decoded:Value = parser.decode(encoded);
        let target:Value = serde_json::from_str(&message).unwrap();
        assert_eq!(target,decoded)
    }

}