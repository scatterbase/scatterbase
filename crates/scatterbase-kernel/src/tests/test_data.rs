//! Test data module for Scatterbase tests
//! 
//! This module provides test data as string constants to avoid relying on filesystem operations in tests.

/// Test environment variables content
pub const TEST_ENV_CONTENT: &str = r#"DOMAIN=http://example.org/
STORAGE_FILE=storage.nt
SERVER_URI=http://example.org/server
ED25519_PUBLIC_KEY_FILE=public.key
ED25519_SECRET_KEY_FILE=private.key
PREFIX_FILE=prefixes.ttl
"#;

/// Test dataset content in N-Triples format
pub const TEST_DATASET_CONTENT: &str = r#"<http://uwlcekqxsvwtoxzm.sandbox> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://schema.org/Dataset> .
<https://scatter.red/triple/odmfxh8yikyf6dtugpwnyzi8aymra3qi5fat1beu96axuj7dhe6y> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/Triple> .
<https://scatter.red/triple/odmfxh8yikyf6dtugpwnyzi8aymra3qi5fat1beu96axuj7dhe6y> <https://scatter.red/terms/v0/claimedIn> <https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/SignedClaim> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <https://scatter.red/terms/v0/includes> <https://scatter.red/triple/odmfxh8yikyf6dtugpwnyzi8aymra3qi5fat1beu96axuj7dhe6y> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <http://www.w3.org/ns/prov#wasAttributedTo> <http://uwlcekqxsvwtoxzm.sandbox/user/alice> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <http://www.w3.org/ns/prov#generatedAtTime> "2025-07-08T00:45:05.121Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <https://scatter.red/terms/v0/seqID> "2025070800450512100"^^<http://www.w3.org/2001/XMLSchema#unsignedLong> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/1> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/2> .
<https://scatter.red/claim/sj31r3jm1oxk5wqctozjer5r83o6orynaaqkmcis1ijxgt7x3iwy> <https://scatter.red/terms/v0/userSignature> "ed25519:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ" .
<https://scatter.red/ack/1> <https://scatter.red/terms/v0/term> <http://uwlcekqxsvwtoxzm.sandbox> .
<https://scatter.red/ack/1> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy> .
<https://scatter.red/ack/2> <https://scatter.red/terms/v0/term> <http://schema.org/Dataset> .
<https://scatter.red/ack/2> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy> .
"#;

/// Test triples content in Turtle format
pub const TEST_TRIPLES_CONTENT: &str = r#"@prefix schema: <http://schema.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix dbpedia: <http://dbpedia.org/resource/> .

dbpedia:Albert_Einstein schema:name "Albert Einstein" ;
                        schema:name "アルベルト・アインシュタイン"@ja ;
                        schema:birthDate "1879-03-14"^^xsd:date ;
                        schema:birthPlace dbpedia:Ulm ;
                        schema:deathDate "1955-04-18"^^xsd:date ;
                        schema:deathPlace <http://dbpedia.org/resource/Princeton,_New_Jersey> ;
                        foaf:homepage <https://einstein-archives.org/> .

dbpedia:Berlin a schema:City ;
               schema:country dbpedia:Germany .

dbpedia:Germany a schema:Country .

dbpedia:Wikipedia a schema:CreativeWork ;
                  schema:inLanguage "en" ;
                  schema:url <https://en.wikipedia.org/wiki/Main_Page> .
"#;

/// Test blank nodes content in Turtle format
pub const TEST_BLANK_NODES_CONTENT: &str = r#"@prefix schema: <http://schema.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<http://example.org/person/alice> a foaf:Person ;
    foaf:name "Alice" ;
    foaf:knows [ a foaf:Person ;
                 foaf:name "Bob" ;
                 schema:birthDate "1990-01-01"^^xsd:date ] .

[] a schema:Event ;
   schema:name "Conference" ;
   schema:startDate "2023-06-15"^^xsd:date ;
   schema:endDate "2023-06-17"^^xsd:date ;
   schema:location [ a schema:Place ;
                     schema:name "Convention Center" ;
                     schema:address [ a schema:PostalAddress ;
                                     schema:streetAddress "123 Main St" ;
                                     schema:addressLocality "Anytown" ] ] .
"#;

/// Mock ED25519 public key for testing
pub const MOCK_PUBLIC_KEY: &str = "ybndrfg8ejkmcpqxot1uwisza345h769ybndrfg8ejkmcpqxot1uwisza345h769";

/// Mock ED25519 private key for testing
pub const MOCK_PRIVATE_KEY: &str = "ybndrfg8ejkmcpqxot1uwisza345h769ybndrfg8ejkmcpqxot1uwisza345h769";

/// Mock prefixes content for testing
pub const MOCK_PREFIXES_CONTENT: &str = r#"@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix schema: <http://schema.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .
"#;

/// RDF-star test content in Turtle format
pub const RDF_STAR_TEST_CONTENT: &str = r#"@prefix ex: <http://example.org/> .
@prefix schema: <http://schema.org/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

ex:alice a foaf:Person ;
    foaf:name "Alice" ;
    foaf:knows ex:bob .

ex:bob a foaf:Person ;
    foaf:name "Bob" .

# Reified triple using RDF-star
<< ex:alice foaf:knows ex:bob >> schema:startDate "2020-01-01"^^<http://www.w3.org/2001/XMLSchema#date> .

# Another reified triple with provenance
<< ex:alice foaf:name "Alice" >> ex:claimedBy ex:alice ;
                                     ex:confidence "0.95"^^<http://www.w3.org/2001/XMLSchema#decimal> .
"#;
