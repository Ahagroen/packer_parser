#![warn(missing_docs)]
//! # Packer-Parser (Name Pending)
//! Encoding and Decoding library for JSONschema based satellite communications schemas
//! Capable of encoding and decoding, with a bidirectional schema (such that the same schema file can be used to both encode and decode messages)
//! 
//! The Aim of this project is to provide a satellite communication standard that is modern and easier to write and develop from than XML based systems. More can be read (here)[] 

use core::panic;
use std::{collections::{HashMap, VecDeque}, fmt, str::from_utf8};
use serde_json::{self, Map, Number, Value};

/// Main interface of the library, created from JSONSchema files
pub struct Parser{
    schema:MultiLayerSchema
}
///Schema representation within the parser. . Bottom layers are the actual subschemas to transmit
#[derive(Clone)]
pub enum MultiLayerSchema{
    ///Layers are the top level schema objects that contain some amount of subschemas
    Layer{
        ///The subschema options from this point. u8 keys are also used as signal bytes for the encoded message
        schemes: Box<HashMap<u8,MultiLayerSchema>>,
        ///The lookup map to map string layer names to u8 keys in the schemes map
        lookup: HashMap<String,u8>,
    },
    ///Final Schema for transmission 
    Bottom(Map<String,Value>)
}
///Error Enumeration for library errors
#[derive(Debug)]
pub enum Error{
    ///Error when parsing a schema file
    ParseError(String),
    ///Error when encoding or decoding data, string param is the keyword where the error occured
    EncodeError{
        ///Description of Error
        error_msg:String,
        ///Keyword where error occured
        error_pos:Option<String>,
    },
}

impl fmt::Display for Error{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self{
            Error::ParseError(reason) => write!(f,"Error when parsing file: {}",reason.to_string()),
            Error::EncodeError { error_msg, error_pos } => write!(f,"Error when processing message at keyword {}: {}",error_pos.clone().unwrap_or("N/A".to_string()).to_string(),error_msg.to_string()),
            
        }
    }
}
fn parse_multilayer_schema(schema:Value)->Result<MultiLayerSchema,Error>{
    //if value has oneOf -> not at bottom level. Parse each element recursively 
    //if value does not have one Of -> at bottom level, return map
    let starting_schema: &Map<String, Value>;
    match schema.as_object(){
        Some(schema) => starting_schema = schema,
        None => return Err(Error::ParseError("Provided Schema is not a valid Key-Value Map".to_string())),
    }
    match starting_schema.get("oneOf"){
        Some(x) => {
            let mut output:HashMap<u8,MultiLayerSchema>=Default::default();
            let subschemes: &Vec<Value>;
            match x.as_array(){
                Some(data) => subschemes = data,
                None => return Err(Error::ParseError("oneOf is incorrectly declared, unable to parse array".to_string())),
            }
            let mut lookup:HashMap<String,u8>=Default::default();
            for (counter, i) in (0_u8..).zip(subschemes.iter()){//is this order consistant
                output.insert(counter,parse_multilayer_schema(i.clone())?);
                let key: String;
                match i.get("id"){
                    Some(key_val) => key = key_val.as_str().expect("Will never panic, loaded as a string").to_string(),
                    None => return Err(Error::ParseError("Could not find subschema with given key".to_string())),
                }
                lookup.insert(key,counter);
            }
            Ok(MultiLayerSchema::Layer { schemes: Box::new(output), lookup })
        },//Recursion
        None => {
            Ok(MultiLayerSchema::Bottom(starting_schema.clone()))
        },//Found the bottom
    }
}
fn find_schema_encoding(scheme:&MultiLayerSchema,message:&Value,mut message_bits_carry:Vec<u8>)->Result<(MultiLayerSchema,Value,Vec<u8>),Error>{
    match scheme{
        MultiLayerSchema::Layer { schemes, lookup } => {
            if message.as_object().unwrap().keys().count() >1{
                return Err(Error::ParseError("Message has more than one signal key".to_string()))
            }
            let signal:&String;
            match message.as_object().unwrap().keys().next(){
                Some(flag) => signal=flag,
                None => return Err(Error::ParseError("Message doesn't have a signal key".to_string())),
            }
            let scheme_id:&u8;
            match lookup.get(signal){
                Some(id) => scheme_id=id,
                None => return Err(Error::EncodeError{error_msg: "Unable to get scheme id".to_string(),error_pos: Some(signal.to_string())}),
            }
            match schemes.get(scheme_id){
                Some(id) => {message_bits_carry.push(*scheme_id);
                    find_schema_encoding(id, message.get(signal).unwrap(),message_bits_carry)
                },
                None => return Err(Error::EncodeError{error_msg: "Unable to get scheme from scheme id".to_string(),error_pos: Some(signal.to_string())}),
            }
            //Should never panic, since 
        },
        MultiLayerSchema::Bottom(_) => {
            Ok((scheme.clone(),message.clone(),message_bits_carry))
        },
    }
}
fn find_schema_decoding(scheme:&MultiLayerSchema,message:&mut VecDeque<u8>,mut message_values_carry:VecDeque<String>)->Result<(MultiLayerSchema,VecDeque<u8>,VecDeque<String>),Error>{
    match scheme{
        MultiLayerSchema::Layer { schemes, lookup } => {
            let signal: u8;
            match message.pop_front(){
                Some(signal_data) => signal = signal_data,
                None => return Err(Error::EncodeError { error_msg: "Message is empty".to_string(), error_pos: None }),
            }
            let sub_scheme: &MultiLayerSchema;
            match schemes.get(&signal){
                Some(data) => sub_scheme = data,
                None => return Err(Error::EncodeError { error_msg: "Provided Message Bit couldn't be found".to_string(), error_pos: Some(signal.to_string()) }),
            }
            for (key,value) in lookup.iter(){
                if *value == signal{
                    message_values_carry.push_back(key.clone())
                }
            }
            find_schema_decoding(sub_scheme, message,message_values_carry)
        },
        MultiLayerSchema::Bottom(_) => {
            Ok((scheme.clone(),message.clone(),message_values_carry))
        },
    }
}

struct MessageConfig{
    order:Vec<Value>,
    scheme:Value,
}
impl MessageConfig{
    fn new(schema:MultiLayerSchema)->Result<MessageConfig,Error>{
        match schema{
            MultiLayerSchema::Layer {.. } => panic!("Didn't return a bottom level scheme"),//Should never happen (Errors are caught before this point)
            MultiLayerSchema::Bottom(x) => {
                Ok(MessageConfig{ order: Self::order(&x)?, scheme: Self::scheme(&x)? })
            },
        }
    }
    fn order(properties:&Map<String,Value>)->Result<Vec<Value>,Error>{//TODO
        let id;
        match properties.get("id"){
            Some(data) => {
                match data.as_str(){
                    Some(data2) => id=data2,
                    None => return Err(Error::ParseError("One of the ID values is not a string".to_string())),
                }
            },
            None => return Err(Error::ParseError("Missing an ID value".to_string())),
        }
        let order: &Vec<Value>;
        match properties.get("required"){
            Some(data) => {
                match data.as_array(){
                    Some(data2) => order=data2,
                    None => return Err(Error::EncodeError{error_msg:"Required Field must be an array".to_string(),error_pos:Some(id.to_string())}),
                }
            },
            None => return Err(Error::EncodeError{error_msg:"Missing Required Field".to_string(),error_pos:Some(id.to_string())}),
        }
        if order.is_empty(){
            match properties.get("properties"){
                Some(data) => {
                    match data.as_object(){
                        Some(data2) => {
                            if data2.is_empty(){
                                return Ok(Vec::new())
                            } else {
                                return Err(Error::EncodeError{error_msg:"Required Field is empty!".to_string(),error_pos:Some(id.to_string())})
                            }
                        },
                        None => return Err(Error::EncodeError{error_msg:"Properties Field is incorrectly formatted".to_string(),error_pos:Some(id.to_string())}),
                    }
                },
                None => return Err(Error::EncodeError{error_msg:"Missing properties Field".to_string(),error_pos:Some(id.to_string())}),
            }
        }

        Ok(order.clone())
    }
    fn scheme(properties:&Map<String,Value>)->Result<Value,Error>{
        let id;
        match properties.get("id"){
            Some(data) => {
                match data.as_str(){
                    Some(data2) => id=data2,
                    None => return Err(Error::ParseError("One of the ID values is not a string".to_string())),
                }
            },
            None => return Err(Error::ParseError("Missing an ID value".to_string())),
        }
        let scheme: Value;
        match properties.get("properties"){
            Some(data) => scheme = data.clone(),//Maybe put properties validation here
            None => return Err(Error::EncodeError{error_msg:"Missing properties Field".to_string(),error_pos:Some(id.to_string())}),
        }
        Ok(scheme)
    }
}
impl Parser{
    ///Creates a new parser from a serde_json value  
    pub fn new(scheme: Value)->Result<Parser,Error>{
        let schema = parse_multilayer_schema(scheme)?;
        Ok(Parser {schema})  
    }

    ///Creates a new parser from a String schema
    pub fn new_from_string(scheme:String)->Result<Parser,Error>{
        let data: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(&scheme);
        match data{
            Ok(value) => Self::new(value),
            Err(_) => Err(Error::ParseError("Schema could not be serialized into key-value map".to_string())),
        }
    }

    ///Encode a given JSON message into vec[u8]
    pub fn encode_from_string(&self,message:&str)->Result<Vec<u8>,Error>{
        let data: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(message);
        match data{
            Ok(value) => self.encode(value),
            Err(_) => Err(Error::ParseError("String could not be serialized into key-value map".to_string())),
        }
    }
    ///Encode a given JSON message into vec[u8]
    pub fn encode(&self,message:Value)->Result<Vec<u8>,Error>{
        //Can assume this is correctly packed
        let (message_conf,pre_processed_message,signal_bit) = find_schema_encoding(&self.schema, &message, vec![])?;
        let message_config = MessageConfig::new(message_conf)?;
        let mut processed_data =vec![signal_bit];
        for i in &message_config.order{
            let unprocessed_data = pre_processed_message.get(i.as_str().unwrap()).unwrap();//Can this fail?
            let current_config = message_config.scheme.get(i.as_str().unwrap()).unwrap().clone();
            let mut output:Vec<u8>;
            match current_config.get("enum"){
                Some(x) => {
                    let data:u8;
                    match x.as_array().unwrap().iter().position(|x| x == unprocessed_data){
                        Some(data2) => data =data2.try_into().expect("More than 256 enum options"),
                        None => return Err(Error::EncodeError { error_msg: "Could not get index of provided enum value".to_string(), error_pos: Some(i.as_str().unwrap().to_string()) }),
                    };
                    output = data.to_le_bytes().to_vec();
                },
                None => {//not enum
                    match current_config.get("type").unwrap().as_str().unwrap(){
                        "boolean" => {
                            match unprocessed_data.as_bool(){
                                Some(data) => {
                                    match data{
                                        true => output = vec![1],
                                        false => output = vec![0],
                                    }
                                },
                                None => return Err(Error::EncodeError { error_msg: "Did not provide a valid boolean".to_string(), error_pos: Some(i.as_str().unwrap().to_string()) }),
                            }
                        },
                        "integer" => {
                            let len:u32 = current_config.get("size").expect("Integer fields must have a declared size").as_u64().expect("Size Must be a number").try_into().expect("Size must be smaller than 32 bits");//Size in bits
                            //should I check the schema before now and assume its valid at this point? Might be more trivial to just check it once and keep these errors to the message
                            let current_data ; 
                            match unprocessed_data.as_i64(){
                                Some(data) => current_data = data,
                                None => return Err(Error::EncodeError { error_msg: "Provided value cannot be deserialized as an integer".to_string(), error_pos: Some(i.as_str().unwrap().to_string())}),
                            }
                            if current_data < 0{
                                if 2_i64.pow(len-1)<current_data{
                                    return Err(Error::EncodeError { error_msg: "Provided value is bigger than maximum".to_string(), error_pos: Some(i.as_str().unwrap().to_string())})
                                }
                            } else if 2_i64.pow(len)<current_data{
                                return Err(Error::EncodeError { error_msg: "Provided value is bigger than maximum".to_string(), error_pos: Some(i.as_str().unwrap().to_string())})
                            }
                                //Then its signed
                            output = current_data.to_le_bytes().split_at((len/8) as usize).0.to_vec();
                            println!("int {:?}",output);
                        },
                        "string" => {
                            let mut carry: Vec<u8>;
                            match unprocessed_data.as_str(){
                                Some(data) => carry = data.as_bytes().to_vec(),
                                None => return Err(Error::EncodeError { error_msg: "Could not serialize data as a string".to_string(), error_pos: Some(i.as_str().unwrap().to_string())}),
                            }
                            let length = carry.len();
                            if length > 256{
                                return Err(Error::EncodeError { error_msg: "Provided string is more than 255 bytes long".to_string(), error_pos: Some(i.as_str().unwrap().to_string())})
                            }
                            output = vec![length as u8];
                            output.append(&mut carry);
                        },
                        "number" => {
                            //Always a 64byte signed float
                            let current_data:f64;
                            match unprocessed_data.as_f64(){
                                Some(x) => current_data = x,
                                None => return Err(Error::EncodeError { error_msg: "Data could not be serialized as a float".to_string(), error_pos: Some(i.as_str().unwrap().to_string())}),
                            }
                            output = current_data.to_le_bytes().to_vec();
                            println!("{:?}",output);
                        },
                        "blob" => {
                            let mut carry: Vec<u8>;
                            match unprocessed_data.as_str(){
                                Some(data) => carry = data.as_bytes().to_vec(),
                                None => return Err(Error::EncodeError { error_msg: "Could not serialize data as a string".to_string(), error_pos: Some(i.as_str().unwrap().to_string())}),
                            }
                            let length = carry.len();
                            if length > 256{
                                return Err(Error::EncodeError { error_msg: "Provided blob is more than 255 bytes long".to_string(), error_pos: Some(i.as_str().unwrap().to_string())})
                            }
                            output = vec![length as u8];
                            output.append(&mut carry);
                        },
                        _ => return Err(Error::EncodeError { error_msg: "Invalid property keyword".to_string(), error_pos: Some(i.as_str().unwrap().to_string())})
                    }
                },
            }
            processed_data.push(output)
        }
    Ok(processed_data.into_iter().flatten().collect())//still need to add pre-append bits
    }
    ///Decode vec[u8] to a string (Formatted as JSON)
    pub fn decode_to_string(&self,message:Vec<u8>)->Result<String,Error>{
        let data = self.decode(message)?;
        Ok(serde_json::to_string(&data).unwrap())
    }
    ///Decode vec[u8] to a serde_json::value Object
    pub fn decode(&self,message: Vec<u8>,)->Result<Value,Error>{
        let mut working_message:VecDeque<u8> = message.into();
        let mut output = serde_json::Map::new();
        let (message_conf,mut working_message,mut signal_values) = find_schema_decoding(&self.schema,&mut working_message,vec![].into())?;
        let message_configs = MessageConfig::new(message_conf)?;
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
                            let len:u32 = current_config.get("size").expect("Integer fields must have a declared size").as_u64().expect("Size Must be a number").try_into().expect("Size must be smaller than 32 bits");//Size in bytes
                            //Again, will this be checked at another point?
                            let mut data:Vec<u8> = working_message.drain(0..len as usize/8).collect();
                            data.reverse();//is this needed
                            while data.len() <8{
                                data.push(0)
                            }
                            let working_output:u64 = u64::from_le_bytes(data.as_slice().try_into().expect("Incorrect Length"));
                            output.insert(i.as_str().unwrap().to_string(),Value::Number(working_output.into()));
                        },
                        "number" => {
                            //always f64
                            let data:Vec<u8> = working_message.drain(0..8).collect();
                            let working_output:f64 = f64::from_le_bytes(data.as_slice().try_into().expect("Incorrect Length"));
                            output.insert(i.as_str().unwrap().to_string(),Value::Number(Number::from_f64(working_output).expect("Couldn't convert to JSON")));
                        },
                        "string" => {
                            let length  = working_message.pop_front().unwrap();
                            let data:Vec<u8> = working_message.drain(0..length as usize).collect();
                            let working_output:String = from_utf8(&data).expect("Can't convert to UTF8").to_string();
                            output.insert(i.as_str().unwrap().to_string(),Value::String(working_output)); 
                        },
                        "blob" => {
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
        Ok(Value::from(Self::create_output_package(output,&mut signal_values)))
    }    
    fn create_output_package(message:Map<String,Value>,frontmatter:&mut VecDeque<String>)->Map<String, Value>{
        if !frontmatter.is_empty(){
            let header = frontmatter.pop_front().expect("Somehow it failed and tried to pop empty");
            let mut data = Map::new();
            data.insert(header, Value::from(Self::create_output_package(message, frontmatter)));
            data
        }
        else{
            message
        } 
    }
    ///Returns a lower level sub scheme as a [MultiLayerSchema] given the top level schema
    pub fn get_schema(&self,top_level_scheme:&String)->MultiLayerSchema{
        match &self.schema{
            MultiLayerSchema::Layer { schemes, lookup } => {
                schemes.get(lookup.get(top_level_scheme).expect("Bad lookup")).expect("Couldn't find scheme").clone()
            },
            MultiLayerSchema::Bottom(_) => panic!("get_schema doesn't make sense in this context"),
        }
    }
    ///Returns all top level schema identifiers
    pub fn get_top_level(&self)->Vec<String>{
        match &self.schema{
            MultiLayerSchema::Layer {lookup,.. } => {
                let top_level:Vec<String> = lookup.keys().cloned().collect();
                top_level
            },
            MultiLayerSchema::Bottom(x) => vec![x.get("id").unwrap().as_str().unwrap().to_string()],
        }
    }
}



#[cfg(test)]
mod tests{
    use std::fs;

    use super::*;
    #[test]
    fn test_loading(){
        Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema file")).unwrap();
        assert!(true)
    }
    #[test]
    fn test_encoding(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema")).unwrap();
        let message = fs::read_to_string(r"src/test_files/Incoming_data.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message).unwrap();
        let expected_message = [0, 50, 4, 84, 101, 115, 116, 1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_decoding(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema")).unwrap();
        let message = [0, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message = parser.decode(message.to_vec()).unwrap();
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_encode_then_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema")).unwrap();
        let message = fs::read_to_string(r"src/test_files/Incoming_data.json").expect("Could not read incoming data file");
        let encoded = parser.encode_from_string(&message).unwrap();
        let decoded:Value = parser.decode(encoded).unwrap();
        let target:Value = serde_json::from_str(&message).unwrap();
        for i in decoded.as_object().unwrap().keys(){
            assert_eq!(decoded.as_object().unwrap().get(i),target.as_object().unwrap().get(i))   
        }
    }
    #[test]
    fn test_multi_schema_encode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message = fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message).unwrap();
        let expected_message = [0, 0, 50, 4, 84, 101, 115, 116, 1, 0, 0, 0, 0, 0, 0, 43, 64];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_two_message_multi_schema_encode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message1 = fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file");
        let encoded_message1 = parser.encode_from_string(&message1).unwrap();
        let expected_message1 = [0, 0, 50, 4, 84, 101, 115, 116, 1, 0, 0, 0, 0, 0, 0, 43, 64];
        let message2 = fs::read_to_string(r"src/test_files/test_command_ack.json").expect("Could not read incoming data file");
        let encoded_message2 = parser.encode_from_string(&message2).unwrap();
        let expected_message2 = [1, 5];
        assert_eq!(encoded_message1,expected_message1);
        assert_eq!(encoded_message2,expected_message2)
    }
    #[test]
    fn test_multi_schema_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message = [0, 0, 50, 4, 84, 101, 115, 116, 1, 0, 0, 0, 0, 0, 0, 43, 64];
        let decoded_message = parser.decode(message.to_vec()).unwrap();
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_two_message_multi_schema_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message1 = [0, 0, 50, 4, 84, 101, 115, 116, 1, 0, 0, 0, 0, 0, 0, 43, 64];
        let decoded_message1 = parser.decode(message1.to_vec()).unwrap();
        let expected_message1:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file")).unwrap();
        let message2 = [1, 5];
        let decoded_message2 = parser.decode(message2.to_vec()).unwrap();
        let expected_message2:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/test_command_ack.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message1,expected_message1);
        assert_eq!(decoded_message2,expected_message2)
    }
    #[test]
    fn test_multi_schema_encode_two_layer(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message = fs::read_to_string(r"src/test_files/Incoming_data_multi_bottom_layer.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message).unwrap();
        let expected_message = [2,0,1,1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_multi_schema_decode_two_layer(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message = [2,0,1,1];
        let decoded_message = parser.decode(message.to_vec()).unwrap();
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_multi_bottom_layer.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_multi_schema_encode_singleton(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message = fs::read_to_string(r"src/test_files/Incoming_data_singleton.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message).unwrap();
        let expected_message = [3];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_multi_schema_decode_singleton(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema")).unwrap();
        let message = [3];
        let decoded_message = parser.decode(message.to_vec()).unwrap();
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_singleton.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
}