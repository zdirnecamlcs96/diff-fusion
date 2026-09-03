// Command pipeline runs the diff-fusion four-step pipeline end to end,
// printing each step's output.
package main

import (
	"fmt"

	"github.com/zdirnecamlcs96/diff-fusion/sdk/golang/examples/jobs"
)

type Doc struct {
	SKU string  `cif:"sku,required" erp:"stock_code"`
	Qty float64 `cif:"qty"          erp:"quantity"`
}

type Entity struct {
	StockCode string  `json:"stock_code"`
	Quantity  float64 `json:"quantity"`
	Note      string  `json:"note"`
}

func main() {
	ancestor := Entity{StockCode: "A1", Quantity: 3, Note: "local"}
	mine := ancestor
	mine.Quantity = 5
	theirs := ancestor
	theirs.StockCode = "A1-R"

	ancestorCIF, err := jobs.TransformIn[Doc]("erp", ancestor)
	must(err)
	mineCIF, err := jobs.TransformIn[Doc]("erp", mine)
	must(err)
	theirsCIF, err := jobs.TransformIn[Doc]("erp", theirs)
	must(err)
	fmt.Printf("1. CIF (mine): %s\n", string(mineCIF))

	changelog, err := jobs.Detect(ancestorCIF, mineCIF, theirsCIF)
	must(err)
	fmt.Printf("2. changelog: %s\n", string(changelog))

	policy := []byte(`{"fields":{"qty":{"kind":"owned_by","system":"erp"},"sku":{"kind":"owned_by","system":"crm"}}}`)
	out, err := jobs.Resolve(jobs.ResolveInput{
		Ancestor:  ancestorCIF,
		Changelog: changelog,
		Policy:    policy,
		SystemA:   "erp",
		SystemB:   "crm",
	})
	must(err)
	fmt.Printf("3. merged: %s\n", string(out.Value))
	fmt.Printf("   conflicts: %s\n", out.Conflicts)

	entity, err := jobs.TransformOut[Doc]("erp", out.Value, ancestor)
	must(err)
	fmt.Printf("4. entity: %+v\n", entity)
}

func must(err error) {
	if err != nil {
		panic(err)
	}
}
