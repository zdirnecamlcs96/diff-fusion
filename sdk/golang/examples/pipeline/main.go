// Command pipeline runs the diff-fusion four-step pipeline end to end for
// two systems — Hubspot and Salesforce — each syncing through its own
// native struct, printing each step's output.
package main

import (
	"fmt"

	"github.com/zdirnecamlcs96/diff-fusion/sdk/golang/examples/jobs"
)

// HubspotContact is Hubspot's own record shape: nested "properties", lower-case keys.
type HubspotContact struct {
	Properties struct {
		Email   string `json:"email" cif:"email,required"`
		Phone   string `json:"phone" cif:"phone"`
		Address struct {
			City string `json:"city" cif:"city"`
			Zip  string `json:"zip"  cif:"zip"`
		} `json:"address" cif:"address"`
	} `json:"properties"`
	HsScore int `json:"hs_score"` // no cif tag: local-only, never crosses
}

// SalesforceContact is Salesforce's own record shape: flat, PascalCase keys.
type SalesforceContact struct {
	Email          string `json:"Email" cif:"email,required"`
	Phone          string `json:"Phone" cif:"phone"`
	MailingAddress struct {
		City       string `json:"City" cif:"city"`
		PostalCode string `json:"PostalCode" cif:"zip"`
	} `json:"MailingAddress" cif:"address"`
	OwnerId string `json:"OwnerId"` // no cif tag: local-only, never crosses
}

func main() {
	// Last-synced Hubspot record: the common ancestor for both sides.
	var lastSynced HubspotContact
	lastSynced.Properties.Email = "jane@example.com"
	lastSynced.Properties.Phone = "555-0100"
	lastSynced.Properties.Address.City = "Springfield"
	lastSynced.Properties.Address.Zip = "00000"

	// Current Hubspot record: phone changed.
	hubspotNow := lastSynced
	hubspotNow.Properties.Phone = "555-0199"
	hubspotNow.HsScore = 87

	// Current Salesforce record: phone changed to a different value (overlapping
	// edit -> conflict), and city changed (disjoint edit -> no conflict).
	var salesforceNow SalesforceContact
	salesforceNow.Email = lastSynced.Properties.Email
	salesforceNow.Phone = "555-0288"
	salesforceNow.MailingAddress.City = "Shelbyville"
	salesforceNow.MailingAddress.PostalCode = lastSynced.Properties.Address.Zip
	salesforceNow.OwnerId = "005xx0000012Abc"

	ancestorCIF, err := jobs.TransformIn("hubspot", lastSynced)
	must(err)
	fmt.Printf("1a. ancestor CIF (hubspot):   %s\n", string(ancestorCIF.RawMessage))

	aCIF, err := jobs.TransformIn("hubspot", hubspotNow)
	must(err)
	fmt.Printf("1b. current CIF (hubspot):    %s\n", string(aCIF.RawMessage))

	bCIF, err := jobs.TransformIn("salesforce", salesforceNow)
	must(err)
	fmt.Printf("1c. current CIF (salesforce): %s\n", string(bCIF.RawMessage))

	changelog, err := jobs.Detect(ancestorCIF, aCIF, bCIF)
	must(err)
	fmt.Printf("2. changelog: %s\n", string(changelog.RawMessage))

	policy := []byte(`{"fields":{"phone":{"kind":"owned_by","system":"hubspot"},"address.city":{"kind":"owned_by","system":"salesforce"}}}`)
	out, err := jobs.Resolve(jobs.ResolveInput{
		Ancestor:  ancestorCIF,
		Changelog: changelog,
		Policy:    policy,
		SystemA:   "hubspot",
		SystemB:   "salesforce",
	})
	must(err)
	fmt.Printf("3. merged: %s\n", string(out.Value.RawMessage))
	fmt.Printf("   conflicts: %s\n", out.Conflicts)

	hubspotOut, err := jobs.TransformOut("hubspot", out.Value, hubspotNow)
	must(err)
	fmt.Printf("4a. Hubspot entity:    %+v\n", hubspotOut)

	salesforceOut, err := jobs.TransformOut("salesforce", out.Value, salesforceNow)
	must(err)
	fmt.Printf("4b. Salesforce entity: %+v\n", salesforceOut)
}

func must(err error) {
	if err != nil {
		panic(err)
	}
}
