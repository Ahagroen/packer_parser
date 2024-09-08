# JSONSchema Based satellite communication scheme

## Introduction

- Intro Sentence
- What a schema is and why it matters
- What metrics judge how useful a schema is
- Potential for something new
- Internet influence
- Thesis

## Rationale

- Deep Dive into XTCE and XML based solutions
- What problems does this have
  - Difficult to use
  - Complex process to generate messages
- Consider other solutions? - are there already JSON versions?
- New solution can provide multifaceted benefits
- Allows for easier generation of scheme, simplifying mission planning
  - Straightforward approach minimizes risk of incorrect scheme definitions or bit positioning
  - Symmetric nature simplifies ground segment development, as both satellite and ground segment can use the same schema system
- JSON datatype for incoming and outgoing data has rich support and provides usability
  - Allows for easy serialization/deserialization of data from the JSON to a hashmap, simplifying the data handling proceedure at both endpoints
  - Commands can be hand-generated and inspected, as they can remain in the JSON format until just before transmission
- New system provides other opportunities for improvements in approach
  - Symmetric system allows for aggressive space optimization, as both sides will be guarenteed to parse every command the same way
  - First class support for enums, ensuring the minimum required bits are used to enumerate all the options (even with a non-round number of bytes)
  - Bit level resolution as required, preventing the need to manually shift/mask bits

## The new solution

- JSONSchema based solution
  - Provides validation of scheme and of messages, ensuring correct messages are passed
  - A number of new keywords are defined beyond the baseline JSONschema system
  - Nested objects provide selection, with the parser descending the tree until it encounters an object that doesn't contain objects.
  - Enums are sized by the parser to ensure a minimum number of required bits to select all possible options, even if that uses a non-round number of bytes
  - Bools are 1bit in length
  - Strings and Blobs (base 64 data) have a size byte as their first byte, indicating the total number of bytes.
    - If the field is the last field of the packet, This size byte is left out and the remaining data from the frame is taken to be the field contents
    - If the field is more than 256 bytes long, a second string field is needed to fill the remaining area (subject to change)
  - Integers and Floats can have their length specified in terms of maximum and minimum possible value, allowing for fine grained control over the length.
    - Very large integers/floats can also be specified by manually providing the number of bytes, and if its signed.
      - This avoids needing to write 2.3m in the schema
  - Arrays are used to influence bit level data positioning
    - An array is the total number of bytes that each contained value requires, without padding
    - As such, 8 boolian values in an array will be 1 byte long
    - This allows for manual implementation of a sign bit, through an array of \[bool, int(unsigned)].
    - Also allows for complex combinations of integers that may be less than one byte long for efficient packing

## The Implementation

In order to effect the solution, two aspects needed to be considered. Firstly, a JSONSchema definition needed to be declared, providing validation and support for the generation of scheme documents, and ensuring compliance with the spec. The second aspect was developing the parsing library that uses these schemes and incoming data to convert between bitstreams/bytestreams and JSON objects.

### JSONSchema

The JSONSchema format is based on _composted_ schemes created out of a base scheme. It enforces definitions for certain keywords such as type, name, and allows for both absolute and conditional keyword definitions. Through this, definitions for keywords can be tuned for the type they are being applied to. Maximum can be defined as a keyword for both integer and string for example, with integer referring to the maximum value and string referring to the maximum length. Through this rich system, complex schemes can be defined.

A satellite TTC packet has a number of critical aspects that needed to be ensured through the schema. Firstly, the order of fields was of utmost importance, so that decoding can be completed using the same schema order. Unfortunitely, as a JSON message is a representation of a HashMap with string keys and values, its order is not consistant. This poses a major problem for decoding, which lead to the decision to enforce a keyword constraint that the _required_ keyword must be present in all bottom level objects (that is, it must occur in every possible packet format). This keyword must be paired to an array, that consists of the parameter names within that packet format. The order of paramaters in the required keyword is taken as the source of truth for the order in the encoded message, and processing proceeds by iterating across this list. This has a number of implications on the usage of the scheme. The order of parameters in the JSON passed to the parsing system is thus neglected and can be any order. In addition, the order of parameters passed out of the parsing system is thus also not guarenteed, and handler implementations downstream from this system must handle the hashmap directly.

### Parameter Lengths

The second critial aspect that needs to be enforced is correct serialization and deserialization of variable length data. When considering known length elements such as bool, and enum where the total number of options is known _a priori_, the encoder is able to utilize the minimum number of bits to encode all possible options, and the decoder can simply read that many bits and be sure to have selected the complete parameter. Fields such as integers, floats, strings and blobs however, may be of a variable length ranging from one byte or less all the way to 64 bytes or more. This poses a problem for the decoder, as determining how long such a field is becomes non-trivial. The first possible solution for this issue is to have the user define within the schema exactly how many bytes the field will be, as well as potentially even if the field is signed or unsigned in the case of integers. If this is known _a priori_, the decoder can simply select those bytes and continue. Not all parameters will have a known length however, and with potentially very large variance in data size that a specific message format may need to handle (such a a file downlink format that may need to download 5 bytes or 5MB), requiring the size to be reserved in advance would add signifigant overhead to the transmission. This lead to the decision to implement a hybrid system, with three possible cases.

The first case is the case discussed previously, where the length is declared in the schema, and the corresponding bytes are always used, regardless of the actual length of the data (padding if needed). This is the least space efficient overall, but in cases where the data is of known length, or where there is very little data so any extra overhead would be more expensive (such as data that is only 1 byte long) this can be used. The second option is indicating the data length within the message. This works by prepending the number of bytes (encoded in 1 extra byte), such that the first byte read informs how many bytes the field is in this instance. One byte of length information provides up to 256 bytes of length, which is still rather short for data transfer. The third case is intended to account for this. To allow for large data transfer, the final case is that the last parameter in the format is considered to take up the remaining data in the message. In this way a 500KiB message can simply have a blob be the last parameter to consider, and thus that parameter can utilize all the remaining space. This is efficient, requiring no extra bytes to complete, and easy to use. It does constrain the position of that free parameter, but as the position in the message is otherwise not likely to be a constraint this decision was deemed to provide the best usability with minimal downsides.  

### Scheme selection

In order to allow schemes to exist in a single file, and eliminate the need to send every possible field with a command, a system for selecting the exact message format to be sent was also required. This was completed using indicator bytes that are prepended to the message before transmission, and read off the beginning of the message during decoding. To indicate a scheme contains sub-schemes, the anyOf keyword is used. This keyword takes an array of schems(objects). As in this instance they are stored in an array, the order is maintained during processing, and the different subschema can be simply indexed with their position in the array. Nesting can be completed by repeating this process. In the actual message, subscheme selection is completed by nesting named JSON objects that correspond to the object in the schema. Through this,  

## Conclusion
