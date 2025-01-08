searchState.loadedDescShard("smart_config", 0, "<code>smart-config</code> – schema-driven layered configuration …\nA wrapper providing a clear reminder that the wrapped …\nContents of a <code>ConfigSource</code>.\nMutable reference to a specific configuration inside …\nParser of configuration input in a <code>ConfigRepository</code>.\nReference to a specific configuration inside <code>ConfigSchema</code>.\nConfiguration repository containing zero or more …\nSchema for configuration. Can contain multiple configs …\nSource of configuration parameters that can be added to a …\nPrioritized list of configuration sources. Can be used to …\nConstructor of <code>Custom</code> types / instances.\nProvides the config description.\nDescribes a configuration (i.e., a group of related …\nDerives the <code>DescribeConfig</code> trait for a type.\nDerives the <code>DeserializeConfig</code> trait for a type.\nMarker error for <code>DeserializeConfig</code> operations. The error …\nConfiguration sourced from environment variables.\nError together with its origin.\nHierarchical configuration.\nJSON-based configuration source.\nKey–value / flat configuration.\nMap of values produced by this source.\nConfig parameter deserialization errors.\nCollection of <code>ParseError</code>s returned from …\nWraps a hierarchical source into a prefix.\nConstructor of <code>Serde</code> types / instances.\nInformation about a source returned from …\nYAML-based configuration source.\nIterates over all aliases for this config.\nIterates over all aliases for this config.\nCreates a value with the specified unit of measurement …\nReturns metadata for the failing config.\nReturns a reference to the configuration.\nCreates <code>Json</code> configuration input based on the provided …\nCreates a custom error.\nConfiguration deserialization logic.\nAccesses options used during <code>serde</code>-powered deserialization.\nCreates an empty JSON source with the specified name.\nFallback <code>Value</code> sources.\nReturns a reference to the first error.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCreates a custom environment.\nGets a reference to a config by ist unique key (metadata + …\nGets a parser for a configuration of the specified type …\nGets a reference to a config by ist unique key (metadata + …\nReturns the wrapped error.\nInner value.\nInserts a new configuration type at the specified place.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nConverts this source into config contents.\nChecks whether this config is top-level (i.e., was …\nIterates over the contained errors.\nIterates over all configs contained in this schema. A …\nIterates over parsers for all configs in the schema.\nReturns the number of contained errors.\nLists all prefixes for the specified config. This does not …\nMerges a value at the specified path into JSON.\nConfiguration metadata.\nGets the config metadata.\nCreates a schema consisting of a single configuration at …\nCreates a source with the specified name and contents.\nCreates a source with the specified name and contents.\nWraps the provided source.\nCreates an empty config repo based on the provided schema.\nCreates a value with the specified unit of measurement.\nReturns an origin of the value deserialization of which …\nOrigin of the value.\nOrigin of the source.\nReturns metadata for the failing parameter if this error …\nNumber of params in the source after it has undergone …\nPerforms parsing.\nParses an optional config. Returns <code>None</code> if the config …\nReturns an absolute path on which this error has occurred.\nGets the config prefix.\nGets the config prefix.\nLoads environment variables with the specified prefix.\nPushes a configuration source at the end of the list.\nPushes an additional alias for the config.\nReturns the wrapped configuration schema.\nReturns a single reference to the specified config.\nReturns a parser for the single configuration of the …\nReturns a single mutable reference to the specified config.\nProvides information about sources merged in this …\nStrips a prefix from all contained vars and returns the …\nTesting tools for configurations.\nEnriched JSON object model that allows to associate values …\nExtends this environment with a new configuration source.\nExtends this environment with a multiple configuration …\nAdds additional variables to this environment. This is …\nCustom deserializer for a specific type. Usually created …\nConstructor of <code>Custom</code> types / instances.\nDeserializer instance.\nDeserializer that supports either an array of values, or a …\nDeserializes this configuration from the provided context.\nContext for deserializing a configuration.\nDeserializes a parameter of the specified type.\nType of the deserializer used for this type.\nAvailable deserialization options.\nDescribes which parameter this deserializer is expecting.\nDeserializer from JSON objects.\nDeserializer for secret strings (any type convertible from …\nDeserializer that supports either a map or an array of …\nDeserializer decorator that wraps the output of the …\nDeserializer that supports parsing either from a default …\nDeserializer decorator that provides additional details …\nDeserializer from JSON arrays.\nDeserializer for arbitrary secret params. Will set the …\nDeserializer powered by <code>serde</code>. Usually created with the …\nConstructor of <code>Serde</code> types / instances.\n<code>Entries</code> instance using the <code>WellKnown</code> deserializers for …\nParameter type with well-known deserializer.\nDeserializer decorator that defaults to the provided value …\nDefault deserializer for <code>Duration</code>s and <code>ByteSize</code>s.\nEnables coercion of variant names between cases, e.g. from …\nReturns a <code>serde</code> deserializer for the current value.\nAdditional info about the deserialized type, e.g., …\nPerforms deserialization.\nPerforms deserialization given the context and param …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nConverts this to a <code>NamedEntries</code> instance.\nCreates a new instance.\nCreates a new deserializer instance with provided key and …\nCreates a new instance with the extended type description.\nGets a string value from the specified env variable.\nFallback source of a configuration param.\nCustom fallback value provider.\nReturns the argument unchanged.\nReturns the argument unchanged.\nGets the raw string value of the env var, taking mock vars …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCreates a provider with the specified human-readable …\nPotentially provides a value for the param.\nAny value.\nArray of values.\nBoolean value.\nSet of one or more basic types in the JSON object model.\nUnit of byte size measurement.\nBase unit – bytes.\nMetadata for a configuration (i.e., a group of related …\nDay (86,400 seconds).\nFloating-point value.\nBinary gigabyte (aka gibibyte) = 1,073,741,824 bytes.\nHour (3,600 seconds).\nInteger value.\nBinary kilobyte (aka kibibyte) = 1,024 bytes.\nBinary megabyte (aka mibibyte) = 1,048,576 bytes.\nMillisecond (0.001 seconds).\nMinute (60 seconds).\nMention of a nested configuration within a configuration.\nObject / map of values.\nMetadata for a specific configuration parameter.\nRepresentation of a Rust type.\nString.\nBase unit – second.\nUnit of byte size measurement.\nUnit of time measurement.\nUnit of time measurement.\nHuman-readable description for a Rust type used in …\nGeneral unit of measurement.\nParam aliases.\nAliases for the config. Cannot be present for flattened …\nChecks whether the <code>needle</code> is fully contained in this set.\nChecks whether this type or any child types (e.g., array …\nReturns the default value for the param.\nGets the type details.\nBasic type(s) expected by the param deserializer.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nHelp regarding the config itself.\nHuman-readable param help parsed from the doc comment.\nReturns the unique ID of this type.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nReturns the description of array items, if one was …\nReturns the description of map keys, if one was provided.\nConfig metadata.\nCanonical param name in the config sources. Not …\nName of the config in config sources. Empty for flattened …\nReturns the name of this type as specified in code.\nNested configs included in the config.\nCreates a new type.\nReturns a union of two sets of basic types.\nParameters included in the config.\nName of the param field in Rust code.\nName of the config field in Rust code.\nRust type of the parameter.\nSets human-readable type details.\nAdds a description of keys and values. This only makes …\nAdds a description of array items. This only makes sense …\nMarks the value as secret.\nAdds a unit of measurement.\nType of this configuration.\nReturns the type description for this param as provided by …\nGets the unit of measurement.\nReturns the description of map values, if one was provided.\nTest case builder that allows configuring deserialization …\nEnables coercion of enum variant names.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nSets mock environment variables that will be recognized by …\nTests config deserialization from the provided <code>sample</code>. …\nTests config deserialization from the provided <code>sample</code>. …\nTests config deserialization ensuring that <em>all</em> declared …\nTests config deserialization ensuring that <em>all</em> declared …\nArray of values.\nBoolean value.\n<code>.env</code> file.\nEnvironment variables.\nFallbacks for config params.\nFile source.\nSupported file formats.\nJSON file.\nJSON object.\n<code>null</code>.\nNumeric value.\nObject / map of values.\nPath from a structured source.\nPlain string value.\nSecret string value.\nSecret string type.\nString value: either a plaintext one, or a secret.\nString value.\nSynthetic value.\nUnknown / default origin.\nJSON value with additional origin information.\nOrigin of a <code>Value</code> in configuration input.\nJSON value together with its origin.\nYAML file.\nAttempts to convert this value to an object.\nAttempts to convert this value to a plain (non-secret) …\nCreates a custom error.\nExposes a secret string if appropriate.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nInner value.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCreates a new value with origin.\nOrigin of the value.\nReturns value at the specified pointer.\nFile format.\nFilename; may not correspond to a real filesystem path.\nDot-separated path in the source, like <code>api.http.port</code>.\nSource of structured data, e.g. a JSON file.\nOriginal value source.\nHuman-readable description of the transform.")