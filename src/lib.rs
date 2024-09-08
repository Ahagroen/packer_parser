use core::panic;
use std::{collections::VecDeque, str::from_utf8};
use serde_json::{self, Map, Value};
use bincode;


pub struct Parser{
    scheme:Value
}
struct MessageConfig{
    order:Vec<Value>,
    scheme:Value,
}
impl Parser{
    pub fn new(scheme: Value)->Parser{//need to come up with a way to feed in a string json
        Parser {scheme}  
    }
    fn order(properties:&Value)->Vec<Value>{
        let order = properties.get("required").expect("Could not find 'required' property, is the scheme correct?").as_array().expect("Required property must be an array").clone();
        return order
    }
    fn encode_configs(&self,message:Value)->(MessageConfig,Value,Option<u8>){
        //Given a multiScheme and String, return the correct sub-scheme, the remaining message and the correct signal bit
        match self.scheme.get("anyOf"){
            Some(x) => {//Multi-scheme
                let data = message.as_object().unwrap();
                if data.len() !=1{
                    panic!("Message has more than one declared top-level schema")
                } else {
                    let req_schema = data.keys().next().expect("Could not get schema name");
                    let message_value = data.get(req_schema).expect("could not access message");
                    let signal_value = x.as_array().expect("anyOf list is not array")
                    .into_iter().position(|x| x.as_object().expect("anyOf did not contain string args")
                    .get("id").expect("No ID Field in Scheme").as_str().unwrap() == req_schema)
                    .expect("Requested Schema not present in scheme doc");
                    let schema = &x.as_array().unwrap()[signal_value];
                    let msgconf = MessageConfig{ order: Self::order(schema), scheme: schema.get("properties").expect("Could not find Properties field").clone() };
                    (msgconf,message_value.clone(),Some(signal_value as u8))
                }
            },//Scheme includes multiple possible message types - first byte used to encode this information
            None => {
                let confg = MessageConfig{ order: Self::order(&self.scheme), scheme: self.scheme.get("properties").expect("Could not find Properties field").clone() };
                (confg,message,None) 
            }
            //Scheme does not include multiple possible message types - first byte is part of message
        }
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
        let (message_configs,pre_processed_message,signal_bit) = self.encode_configs(message);
        let mut processed_data = vec![];
        match signal_bit{
            Some(x) => processed_data.push(vec![x]),
            None => (),
        }
        for i in &message_configs.order{
            let unprocessed_data = pre_processed_message.get(i.as_str().unwrap()).unwrap();
            let current_config = message_configs.scheme.get(i.as_str().unwrap()).unwrap().clone();
            let output:Vec<u8>;
            match current_config.get("enum"){
                Some(x) => {
                    let data:Value =x.as_array().unwrap().into_iter().position(|x| x == unprocessed_data).expect("Could not get index of enum value").into();
                    output = Self::to_bytes(&data,1)//need to validate no more than 1 byte worth of enum variants - or read the potential size of the enum
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
                        "base64" => output = Self::to_bytes(unprocessed_data, 0),//Do I want determined_length strings?
                        // "object" => {
                        //     ()
                        // }
                        _ => panic!("Cannot parse")
                    }
                },
            }
            processed_data.push(output)
        }
    processed_data.into_iter().flatten().collect()//still need to add pre-append bits
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


    pub fn decode_to_string(&self,message:Vec<u8>)->String{
        let data = self.decode(message);
        return serde_json::to_string(&data).unwrap();
    }
    fn decode_configs(&self,message:&mut VecDeque<u8>)->(MessageConfig,Option<String>){
        match self.scheme.get("anyOf"){
            Some(x) => {
                let signal_byte = message.pop_front().expect("Message is empty");
                let schema = &x.as_array().unwrap()[signal_byte as usize];
                let msgconf = MessageConfig{ order: Self::order(schema), scheme: schema.get("properties").expect("Could not find Properties field").clone() };
                let code = schema.get("id").expect("No id field in schema");
                (msgconf,Some(code.as_str().unwrap().to_string()))
            },//Scheme contains sub-schema, return MessageConfig and remaining message with first bit removed
            None => {
                (MessageConfig{ order: Self::order(&self.scheme), scheme: self.scheme.get("properties").expect("Could not find Properties field").clone(),}, None)
            },//Scheme does not work on a sub-schema, return 
        }
    }
    pub fn decode(&self,message: Vec<u8>,)->Value{//still need to handle preappend bits
        let mut working_message:VecDeque<u8> = message.into();
        let mut output = serde_json::Map::new();
        let (message_configs,msgtype) = self.decode_configs(&mut working_message);
        for i in message_configs.order{
            let current_config = message_configs.scheme.get(i.as_str().unwrap()).unwrap().clone();
            match current_config.get("enum"){
                Some(x) => {
                    let data:u8 = working_message.pop_front().unwrap();
                    output.insert(i.as_str().unwrap().to_string(),x.as_array().unwrap().get(data as usize).unwrap().clone());
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
        match msgtype{
            Some(x) => {
                let mut prefixed_output = serde_json::Map::new();
                prefixed_output.insert(x, Value::Object(output));
                Value::from(prefixed_output)
            },
            None => Value::from(output),
        }
         //Need to convert back into JSON
    }    
}

#[derive(Clone)]
struct Schema{
    front_matter:Vec<String>,
    main:Map<String,Value>
}
fn parse_schema(schema:Value,front_matter:Vec<String>)->Vec<Schema>{
    //if value has oneOf -> not at bottom level. Parse each element recursively 
    //if value does not have one Of -> at bottom level, return map
    let starting_schema = schema.as_object().expect("Not an object");
    match starting_schema.get("oneOf"){
        Some(x) => {
            let mut output:Vec<Schema> = vec![];
            let subschemes = x.as_array().expect("Invalid formatting for oneOF");
            for i in subschemes{
                let mut front = front_matter.clone();
                front.push(starting_schema.get("id").expect("Could not find ID").to_string());
                output = [output,parse_schema(i.clone(), front)].concat();
            }
            output
        },//Recursion
        None => {
            return vec![Schema{ front_matter, main: starting_schema.clone() }]
        },//Found the bottom
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
        let expected_message = [0, 50, 4, 84, 101, 115, 116, 1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_decoding(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\scheme.json").expect("Could not read schema"));
        let message = [0, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message = parser.decode(message.to_vec());
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src\test_files\Incoming_data.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_encode_then_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\scheme.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src\test_files\Incoming_data.json").expect("Could not read incoming data file");
        let encoded = parser.encode_from_string(&message);
        let decoded:Value = parser.decode(encoded);
        let target:Value = serde_json::from_str(&message).unwrap();
        for i in decoded.as_object().unwrap().keys(){
            assert_eq!(decoded.as_object().unwrap().get(i),target.as_object().unwrap().get(i))   
        }
    }
    #[test]
    fn test_multi_schema_encode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\multi_schema_test.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src\test_files\Incoming_data_multi.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message);
        let expected_message = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_two_message_multi_schema_encode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\multi_schema_test.json").expect("Could not read schema"));
        let message1 = fs::read_to_string(r"src\test_files\Incoming_data_multi.json").expect("Could not read incoming data file");
        let encoded_message1 = parser.encode_from_string(&message1);
        let expected_message1 = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        let message2 = fs::read_to_string(r"src\test_files\test_command_ack.json").expect("Could not read incoming data file");
        let encoded_message2 = parser.encode_from_string(&message2);
        let expected_message2 = [1, 5];
        assert_eq!(encoded_message1,expected_message1);
        assert_eq!(encoded_message2,expected_message2)
    }
    #[test]
    fn test_multi_schema_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\multi_schema_test.json").expect("Could not read schema"));
        let message = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message = parser.decode(message.to_vec());
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src\test_files\Incoming_data_multi.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_two_message_multi_schema_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src\test_files\multi_schema_test.json").expect("Could not read schema"));
        let message1 = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message1 = parser.decode(message1.to_vec());
        let expected_message1:Value = serde_json::from_str(&fs::read_to_string(r"src\test_files\Incoming_data_multi.json").expect("Could not read incoming data file")).unwrap();
        let message2 = [1, 5];
        let decoded_message2 = parser.decode(message2.to_vec());
        let expected_message2:Value = serde_json::from_str(&fs::read_to_string(r"src\test_files\test_command_ack.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message1,expected_message1);
        assert_eq!(decoded_message2,expected_message2)
    }
}