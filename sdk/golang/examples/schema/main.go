// Command schema derives a CIF schema from a tagged Go struct with
// kernel.SchemaFromStruct and feeds it to TransformToCIF.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"log"
	"time"

	"github.com/zdirnecamlcs96/diff-fusion/sdk/golang/kernel"
)

// HubspotContact is Hubspot's own record shape and exercises every
// SchemaFromStruct derivation rule.
type HubspotContact struct {
	Properties struct {
		// json tag is the source key; cif tag (with ",required") names the CIF field.
		Email string `json:"email" cif:"email,required"`
		// optional CIF field.
		Phone string `json:"phone" cif:"phone"`
		// nested struct, cif-tagged: its own CIF object; children's paths are relative to it.
		Address struct {
			City string `json:"city" cif:"city"`
			Zip  string `json:"zip"  cif:"zip"`
		} `json:"address" cif:"address"`
	} `json:"properties"` // no cif tag: transparent — children's source paths are prefixed "properties."
	// slice of struct, cif-tagged: a CIF array; element source paths are relative to each element.
	Tags []struct {
		Name string `json:"name" cif:"label"`
	} `json:"tags" cif:"tags"`
	// no cif tag: local-only, not part of the CIF schema at all.
	HsScore int `json:"hs_score"`
}

// badContact triggers SchemaFromStruct's time.Time rejection.
type badContact struct {
	CreatedAt time.Time `json:"createdate" cif:"created_at"`
}

func main() {
	schema, err := kernel.SchemaFromStruct(new(HubspotContact), "hubspot")
	if err != nil {
		log.Fatal(err)
	}
	var pretty bytes.Buffer
	if err := json.Indent(&pretty, schema, "", "  "); err != nil {
		log.Fatal(err)
	}
	fmt.Println("--- schema ---")
	fmt.Println(pretty.String())

	ctx := context.Background()
	k, err := kernel.New(ctx)
	if err != nil {
		log.Fatal(err)
	}
	defer k.Close(ctx)

	entity := []byte(`{
		"properties": {
			"email": "jane@example.com",
			"phone": "555-0100",
			"address": {"city": "Springfield", "zip": "00000"}
		},
		"tags": [{"name": "beta"}],
		"hs_score": 42
	}`)
	cif, err := k.TransformToCIF(ctx, entity, schema, "hubspot")
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("--- cif ---")
	fmt.Println(string(cif))

	_, err = kernel.SchemaFromStruct(new(badContact), "hubspot")
	fmt.Println("--- rejected ---")
	fmt.Println(err)
}
