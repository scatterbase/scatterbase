use std::io::BufReader;
use std::collections::HashMap;

use crate::triple::TripleExt;
use crate::claim::parse_turtle;
use oxrdf::Term;
use crate::tests::test_data::TEST_TRIPLES_CONTENT;

#[test]
fn test_parse_triples() {
    // Use in-memory test data instead of reading from a file
    let file_content = TEST_TRIPLES_CONTENT;
    println!("File content:\n{}", file_content);
    let reader = BufReader::new(file_content.as_bytes());

    // Create a prefixes map
    let mut prefixes = HashMap::new();
    prefixes.insert("schema".to_string(), "http://schema.org/".to_string());
    prefixes.insert("foaf".to_string(), "http://xmlns.com/foaf/0.1/".to_string());
    prefixes.insert("xsd".to_string(), "http://www.w3.org/2001/XMLSchema#".to_string());
    prefixes.insert("dbpedia".to_string(), "http://dbpedia.org/resource/".to_string());

    // Parse the Turtle file
    let triples = parse_turtle(reader, &prefixes).expect("Failed to parse Turtle file");

    // Print the triples for debugging
    println!("Parsed {} triples:", triples.len());
    for triple in &triples {
        println!("{}", triple.to_ntriples());

        // More detailed debugging for all triples
        let subject = triple.subject.to_string();
        let property = triple.predicate.as_str().to_string();
        let object = &triple.object;

        println!("DEBUG: Subject: '{}', Property: '{}', Object: '{}'", subject, property, object);

        // More detailed debugging for Einstein name triples
        if subject == "http://dbpedia.org/resource/Albert_Einstein" 
           && property == "http://schema.org/name" {
            if let Term::Literal(literal) = object {
                println!("DEBUG: Found Einstein name: '{}', language_tag: {:?}", 
                         literal.value(), literal.language());
            }
        }
    }

    // Verify that the basic triples were parsed correctly
    let mut found_einstein_name = false;
    let mut found_einstein_japanese_name = false;
    let mut found_einstein_birthdate = false;
    let mut found_einstein_birthplace = false;
    let mut found_einstein_deathdate = false;
    let mut found_einstein_deathplace = false;
    let mut found_einstein_homepage = false;
    let mut found_berlin_city = false;
    let mut found_berlin_country = false;
    let mut found_germany_country = false;
    let mut found_wikipedia_creative_work = false;
    let mut found_wikipedia_language = false;
    let mut found_wikipedia_url = false;

    for triple in &triples {
        let subject = triple.subject.to_string();
        let property = triple.predicate.as_str().to_string();
        let object = &triple.object;

        // Print more detailed information about each triple for debugging
        println!("Checking triple - Subject: {}, Property: {}, Object type: {:?}", 
                 subject, property, object);

        if subject == "<http://dbpedia.org/resource/Albert_Einstein>" {
            if property == "http://schema.org/name" {
                if let Term::Literal(literal) = object {
                    println!("Found Einstein name: '{}', language_tag: {:?}", 
                             literal.value(), literal.language());
                    if literal.value() == "Albert Einstein" {
                        found_einstein_name = true;
                        println!("Matched Einstein English name!");
                    } else if literal.value() == "アルベルト・アインシュタイン" && literal.language().unwrap_or_default() == "ja" {
                        found_einstein_japanese_name = true;
                        println!("Matched Einstein Japanese name!");
                    }
                }
            } else if property == "http://schema.org/birthDate" {
                if let Term::Literal(literal) = object {
                    if literal.value() == "1879-03-14" {
                        found_einstein_birthdate = true;
                    }
                }
            } else if property == "http://schema.org/birthPlace" {
                if object.to_string() == "<http://dbpedia.org/resource/Ulm>" {
                    found_einstein_birthplace = true;
                }
            } else if property == "http://schema.org/deathDate" {
                if let Term::Literal(literal) = object {
                    if literal.value() == "1955-04-18" {
                        found_einstein_deathdate = true;
                    }
                }
            } else if property == "http://schema.org/deathPlace" {
                if object.to_string() == "<http://dbpedia.org/resource/Princeton,_New_Jersey>" {
                    found_einstein_deathplace = true;
                }
            } else if property == "http://xmlns.com/foaf/0.1/homepage" {
                if object.to_string() == "<https://einstein-archives.org/>" {
                    found_einstein_homepage = true;
                }
            }
        } else if subject == "<http://dbpedia.org/resource/Berlin>" {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                if object.to_string() == "<http://schema.org/City>" {
                    found_berlin_city = true;
                }
            } else if property == "http://schema.org/country" {
                if object.to_string() == "<http://dbpedia.org/resource/Germany>" {
                    found_berlin_country = true;
                }
            }
        } else if subject == "<http://dbpedia.org/resource/Germany>" {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                if object.to_string() == "<http://schema.org/Country>" {
                    found_germany_country = true;
                }
            }
        } else if subject == "<http://dbpedia.org/resource/Wikipedia>" {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                if object.to_string() == "<http://schema.org/CreativeWork>" {
                    found_wikipedia_creative_work = true;
                }
            } else if property == "http://schema.org/inLanguage" {
                if let Term::Literal(literal) = object {
                    if literal.value() == "en" {
                        found_wikipedia_language = true;
                    }
                }
            } else if property == "http://schema.org/url" {
                if object.to_string() == "<https://en.wikipedia.org/wiki/Main_Page>" {
                    found_wikipedia_url = true;
                }
            }
        }
    }

    // Verify that all triples were found
    assert!(found_einstein_name, "Missing triple: dbpedia:Albert_Einstein schema:name \"Albert Einstein\"");
    assert!(found_einstein_japanese_name, "Missing triple: dbpedia:Albert_Einstein schema:name \"アルベルト・アインシュタイン\"@ja");
    assert!(found_einstein_birthdate, "Missing triple: dbpedia:Albert_Einstein schema:birthDate \"1879-03-14\"^^xsd:date");
    assert!(found_einstein_birthplace, "Missing triple: dbpedia:Albert_Einstein schema:birthPlace dbpedia:Ulm");
    assert!(found_einstein_deathdate, "Missing triple: dbpedia:Albert_Einstein schema:deathDate \"1955-04-18\"^^xsd:date");
    assert!(found_einstein_deathplace, "Missing triple: dbpedia:Albert_Einstein schema:deathPlace dbpedia:Princeton,_New_Jersey");
    assert!(found_einstein_homepage, "Missing triple: dbpedia:Albert_Einstein foaf:homepage <https://einstein-archives.org/>");
    assert!(found_berlin_city, "Missing triple: dbpedia:Berlin a schema:City");
    assert!(found_berlin_country, "Missing triple: dbpedia:Berlin schema:country dbpedia:Germany");
    assert!(found_germany_country, "Missing triple: dbpedia:Germany a schema:Country");
    assert!(found_wikipedia_creative_work, "Missing triple: dbpedia:Wikipedia a schema:CreativeWork");
    assert!(found_wikipedia_language, "Missing triple: dbpedia:Wikipedia schema:inLanguage \"en\"");
    assert!(found_wikipedia_url, "Missing triple: dbpedia:Wikipedia schema:url <https://en.wikipedia.org/wiki/Main_Page>");
}
