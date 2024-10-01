# Basic Schema Definition
The definition of schema files is designed to match the JSONSchema specification approach, and each schema file must also be valid JSON for parsing. 

The overall schema is defined at the top level with a "id" value naming the overall schema, as well as a "version" keyword with value 1 (For this version of the scheme). Then either a packet definition or an anyOf list is specified Each layer is wrapped in curly braces unless specified otherwise. 

Schemas are bidirectional, such that the parsing library can utilize the same schema file to both encode and decode data. In this way utilization of the schema is simplified, by allowing both the OBC and the ground segment to seamlessly transfer key-value pairs directly with highly efficient encoding.


## anyOf
Specifies a set of potential packet definitions. Defined as an array of options. Encoded as index value, using the minimum number of bytes to represent all options (Having more than 255 different options in a single layer is discouraged)
- Declaration: 
    1. "id" keyword with string identifier
    2. "anyOf" keyword with array value

## Packet Definition
Specifies a final packet definition. All fields within this definition are required to be included in the sent packet
- Declaration:
    1. "id" keyword with string identifer
    2. "required" keyword with array containing the _keyword name_ of each parameter in the packet
        - Specifies the order of parameters for encoding
    3. "type" keyword with value "object"
    4. "properties" keyword with curly braces (object) value that contains the parameters of the command
Note that for a command with no parameters (for example starting a pass), the required keyword must still be specified with an empty list, and the properties keyword is an empty object.

## Parameters
- Each parameter must be wrapped by the properties keyword, and its name must be included in the required parameter for it to be sent. Optional parameters are not supported
- Each property is defined by declaring the name of the field, then setting the value to the name keyword as an object
- A paremeter can either be a type - in which case the value is encoded and sent, or an enumerated option - in which case the value is compared to the list of values and the _index_ is sent. 

### Enums
Enums provide space efficient methods of sending common data such as satellite state or deployment state of a component.
- Enums are defined as a parameter, then within the curly braces:
    1. "enum" keyword - array of options (square brackets containing the possible options)
    2. "description" keyword (optional) - Describes the field 

### Types
Types include boolean, integer, number, string and blob. 

#### Boolean
A boolean value (T/F). Currently encoded using 1 Byte but bit level encoding is planned for V2
- Defined as a named parameter, then:
    1. "type" keyword - "boolean"
    2. "description" keyword (optional) - Describes the field 

#### Integer
An Integer of variable size. Currently only support for round byte sizing but bit level granularity is planned for V2
- Defined as a named parameter, then:
    1. "type" keyword - "integer"
    2. "size" parameter - max size in bits
        - NOTE: Must compute to a round number of bytes (and thus be divisable by 8)
        - NOTE: All bytes are allocated in the packet regardless of passed value
    3. "description" keyword (optional) - Describes the field 

#### Number
A double precision floating point number (64 bits). Support for variable length floats is a long term goal, but not currently planned
- Defined as a name paremeter, then:
    1. "type" keyword - "number"
    2. "description" keyword (optional) - Describes the field 

#### String
Defined as a variable length string. Maximum permitted length is 256 bytes of UTF-8 encoding. The length is encoded within the final bytestream, so only allocates the required length of the passed message + 1 byte. If the string field is specified last in the list provided to the required keyword, the length byte is not included and the string is assumed to use the remaining packet space (256 byte limit still applies)
- Defined as a named parameter, then:
    1. "type" keyword - "string"
    2. "description" keyword (optional) - Describes the field 
 
#### Blob
Variable length data field. Implemented as a string in the parsing definition, and thus shares the same 256 byte maximum. Post-processing is required as blob returns valid UTF-8 after decoding. First class blob handling is planned for V2
- Defined as a named parameter, then: 
    1. "type" keyword - "string"
    2. "description" keyword (optional) - Describes the field 
    