use core::panic;
use std::{collections::{HashMap, VecDeque}, str::from_utf8};
use serde_json::{self, Map, Value};
use bincode;

pub struct Parser{
    schema:MultiLayerSchema
}
#[derive(Clone)]
pub enum MultiLayerSchema{
    Layer{
        schemes: Box<HashMap<u8,MultiLayerSchema>>,
        lookup: HashMap<String,u8>,
    },
    Bottom(Map<String,Value>)
}

fn parse_multilayer_schema(schema:Value)->MultiLayerSchema{
    //if value has oneOf -> not at bottom level. Parse each element recursively 
    //if value does not have one Of -> at bottom level, return map
    let starting_schema = schema.as_object().expect("Not an object");
    match starting_schema.get("oneOf"){
        Some(x) => {
            let mut output:HashMap<u8,MultiLayerSchema>=Default::default();
            let subschemes = x.as_array().expect("Invalid formatting for oneOF");
            let mut counter:u8 = 0;
            let mut lookup:HashMap<String,u8>=Default::default();
            for i in subschemes{//is this order consistant
                output.insert(counter,parse_multilayer_schema(i.clone()));
                lookup.insert(i.get("id").expect("Could not find ID").as_str().unwrap().to_string(),counter);
                counter +=1;
            }
            MultiLayerSchema::Layer { schemes: Box::new(output), lookup }
        },//Recursion
        None => {
            return MultiLayerSchema::Bottom(starting_schema.clone())
        },//Found the bottom
    }
}
fn find_schema_encoding(scheme:&MultiLayerSchema,message:&Value,mut message_bits_carry:Vec<u8>)->(MultiLayerSchema,Value,Vec<u8>){
    match scheme{
        MultiLayerSchema::Layer { schemes, lookup } => {
            if message.as_object().unwrap().keys().count() >1{
                panic!("More than one top level schema defined in the message")
            }
            let signal =  message.as_object().unwrap().keys().next().expect("Couldn't find ID");
            println!("{:?}",signal);
            let scheme_id = lookup.get(signal).expect("id not recognized - String");
            let sub_scheme = schemes.get(scheme_id).expect("id not recognized - usize");
            message_bits_carry.push(*scheme_id);
            return find_schema_encoding(sub_scheme, message.get(signal).unwrap(),message_bits_carry)//Should never panic, since 
        },
        MultiLayerSchema::Bottom(_) => {
            return (scheme.clone(),message.clone(),message_bits_carry)
        },
    }
}
fn find_schema_decoding(scheme:&MultiLayerSchema,message:&mut VecDeque<u8>,mut message_values_carry:VecDeque<String>)->(MultiLayerSchema,VecDeque<u8>,VecDeque<String>){
    match scheme{
        MultiLayerSchema::Layer { schemes, lookup } => {
            let signal =  message.pop_front().expect("Message was empty!");
            let sub_scheme = schemes.get(&signal).expect("id not recognized - usize");
            for (key,value) in lookup.iter(){
                if *value == signal{
                    message_values_carry.push_back(key.clone())
                }
            }
            return find_schema_decoding(sub_scheme, message,message_values_carry)
        },
        MultiLayerSchema::Bottom(_) => {
            return (scheme.clone(),message.clone(),message_values_carry)
        },
    }
}

struct MessageConfig{
    order:Vec<Value>,
    scheme:Value,
}
impl MessageConfig{
    fn new(schema:MultiLayerSchema)->MessageConfig{
        match schema{
            MultiLayerSchema::Layer {.. } => panic!("Didn't return a bottom level scheme"),
            MultiLayerSchema::Bottom(x) => {
                MessageConfig{ order: Self::order(&x), scheme: Self::scheme(&x) }
            },
        }
    }
    fn order(properties:&Map<String,Value>)->Vec<Value>{
        let order = properties.get("required").expect("Could not find 'required' property, is the scheme correct?").as_array().expect("Required property must be an array");
        if order.len() == 0{
            if properties.get("properties").expect("could not find the properties field").as_object().unwrap().is_empty(){
                return Vec::new()
            }
            panic!("Required field is empty: Must contain all relevent fields in the proper order.")
        }

        return order.clone()
    }
    fn scheme(properties:&Map<String,Value>)->Value{
        let scheme = properties.get("properties").expect("Could not find Properties field").clone();
        return scheme
    }
}
impl Parser{
    pub fn new(scheme: Value)->Parser{//need to come up with a way to feed in a string json
        let schema = parse_multilayer_schema(scheme);
        Parser {schema}  
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
        let (message_conf,pre_processed_message,signal_bit) = find_schema_encoding(&self.schema, &message, vec![]);
        let message_config = MessageConfig::new(message_conf);
        let mut processed_data =vec![signal_bit];
        for i in &message_config.order{
            let unprocessed_data = pre_processed_message.get(i.as_str().unwrap()).unwrap();
            let current_config = message_config.scheme.get(i.as_str().unwrap()).unwrap().clone();
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
    pub fn decode(&self,message: Vec<u8>,)->Value{
        let mut working_message:VecDeque<u8> = message.into();
        let mut output = serde_json::Map::new();
        let (message_conf,mut working_message,mut signal_values) = find_schema_decoding(&self.schema,&mut working_message,vec![].into());
        let message_configs = MessageConfig::new(message_conf);
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
        Value::from(Self::create_output_package(output,&mut signal_values))
         //Need to convert back into JSON
    }    
    fn create_output_package(message:Map<String,Value>,frontmatter:&mut VecDeque<String>)->Map<String, Value>{
        if frontmatter.len() > 0{
            let header = frontmatter.pop_front().expect("Somehow if failed and tried to pop empty");
            let mut data = Map::new();
            data.insert(header, Value::from(Self::create_output_package(message, frontmatter)));
            return data
        }
        else{
            message
        } 
    }
    pub fn get_schema(&self,top_level_scheme:&String)->MultiLayerSchema{
        match &self.schema{
            MultiLayerSchema::Layer { schemes, lookup } => {
                schemes.get(lookup.get(top_level_scheme).expect("Bad lookup")).expect("Couldn't find scheme").clone()
            },
            MultiLayerSchema::Bottom(_) => panic!("get_schema doesn't make sense in this context"),
        }
    }
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
        Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema file"));
        assert!(true)
    }
    #[test]
    fn test_encoding(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src/test_files/Incoming_data.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message);
        let expected_message = [0, 50, 4, 84, 101, 115, 116, 1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_decoding(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema"));
        let message = [0, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message = parser.decode(message.to_vec());
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_encode_then_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/scheme.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src/test_files/Incoming_data.json").expect("Could not read incoming data file");
        let encoded = parser.encode_from_string(&message);
        let decoded:Value = parser.decode(encoded);
        let target:Value = serde_json::from_str(&message).unwrap();
        for i in decoded.as_object().unwrap().keys(){
            assert_eq!(decoded.as_object().unwrap().get(i),target.as_object().unwrap().get(i))   
        }
    }
    #[test]
    fn test_multi_schema_encode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message);
        let expected_message = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_two_message_multi_schema_encode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message1 = fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file");
        let encoded_message1 = parser.encode_from_string(&message1);
        let expected_message1 = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        let message2 = fs::read_to_string(r"src/test_files/test_command_ack.json").expect("Could not read incoming data file");
        let encoded_message2 = parser.encode_from_string(&message2);
        let expected_message2 = [1, 5];
        assert_eq!(encoded_message1,expected_message1);
        assert_eq!(encoded_message2,expected_message2)
    }
    #[test]
    fn test_multi_schema_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message = parser.decode(message.to_vec());
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_two_message_multi_schema_decode(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message1 = [0, 0, 50, 4, 84, 101, 115, 116, 1];
        let decoded_message1 = parser.decode(message1.to_vec());
        let expected_message1:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_multi.json").expect("Could not read incoming data file")).unwrap();
        let message2 = [1, 5];
        let decoded_message2 = parser.decode(message2.to_vec());
        let expected_message2:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/test_command_ack.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message1,expected_message1);
        assert_eq!(decoded_message2,expected_message2)
    }
    #[test]
    fn test_multi_schema_encode_two_layer(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src/test_files/Incoming_data_multi_bottom_layer.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message);
        let expected_message = [2,0,1,1];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_multi_schema_decode_two_layer(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message = [2,0,1,1];
        let decoded_message = parser.decode(message.to_vec());
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_multi_bottom_layer.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
    #[test]
    fn test_multi_schema_encode_singleton(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message = fs::read_to_string(r"src/test_files/Incoming_data_singleton.json").expect("Could not read incoming data file");
        let encoded_message = parser.encode_from_string(&message);
        let expected_message = [3];
        assert_eq!(encoded_message,expected_message)
    }
    #[test]
    fn test_multi_schema_decode_singleton(){
        let parser = Parser::new_from_string(fs::read_to_string(r"src/test_files/multi_schema_test.json").expect("Could not read schema"));
        let message = [3];
        let decoded_message = parser.decode(message.to_vec());
        let expected_message:Value = serde_json::from_str(&fs::read_to_string(r"src/test_files/Incoming_data_singleton.json").expect("Could not read incoming data file")).unwrap();
        assert_eq!(decoded_message.as_object().unwrap(),expected_message.as_object().unwrap())
    }
}