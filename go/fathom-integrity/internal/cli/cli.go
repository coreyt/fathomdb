package cli

import (
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"strings"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/commands"
	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/config"
)

// Main parses CLI arguments, dispatches to the appropriate subcommand, and
// returns the process exit code.
func Main(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprintln(stderr, usage())
		return 2
	}

	cfg := config.Load()

	switch args[0] {
	case "check":
		fs := flag.NewFlagSet("check", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", "", "path to admin bridge binary (optional; enables Layer 2 engine checks)")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		if err := commands.RunCheckWithFeedback(
			*db,
			*bridgeBinary,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "export":
		fs := flag.NewFlagSet("export", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		destination := fs.String("out", "", "path to export destination")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		forceCheckpoint := fs.Bool(
			"force-checkpoint",
			false,
			"request a full WAL checkpoint before export; stricter but may fail while readers are active",
		)
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *destination == "" {
			fmt.Fprintln(stderr, "--db and --out are required")
			return 2
		}
		if err := commands.RunExportWithFeedback(
			*db,
			*destination,
			*bridgeBinary,
			*forceCheckpoint,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "trace":
		fs := flag.NewFlagSet("trace", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		sourceRef := fs.String("source-ref", "", "source reference to trace")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *sourceRef == "" {
			fmt.Fprintln(stderr, "--db and --source-ref are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandTraceSource,
			SourceRef:    *sourceRef,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "restore-logical-id":
		fs := flag.NewFlagSet("restore-logical-id", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		logicalID := fs.String("logical-id", "", "logical id to restore")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *logicalID == "" {
			fmt.Fprintln(stderr, "--db and --logical-id are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandRestoreLogicalID,
			LogicalID:    *logicalID,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "purge-logical-id":
		fs := flag.NewFlagSet("purge-logical-id", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		logicalID := fs.String("logical-id", "", "retired logical id to purge")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *logicalID == "" {
			fmt.Fprintln(stderr, "--db and --logical-id are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandPurgeLogicalID,
			LogicalID:    *logicalID,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "trace-operational":
		fs := flag.NewFlagSet("trace-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to trace")
		recordKey := fs.String("record-key", "", "optional record key to narrow the trace")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandTraceOperationalCollection,
			CollectionName: *collectionName,
			RecordKey:      *recordKey,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "read-operational":
		fs := flag.NewFlagSet("read-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to read")
		filtersJSON := fs.String("filters-json", "", "JSON array of declared operational filter clauses")
		limit := fs.Int("limit", 0, "optional maximum number of rows to return")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" || *filtersJSON == "" {
			fmt.Fprintln(stderr, "--db, --collection, and --filters-json are required")
			return 2
		}
		var filters []bridge.OperationalFilterClause
		if err := json.Unmarshal([]byte(*filtersJSON), &filters); err != nil {
			fmt.Fprintf(stderr, "invalid --filters-json: %v\n", err)
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandReadOperationalCollection,
			CollectionName: *collectionName,
			OperationalRead: &bridge.OperationalReadRequest{
				CollectionName: *collectionName,
				Filters:        filters,
				Limit:          *limit,
			},
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "update-operational-filters":
		fs := flag.NewFlagSet("update-operational-filters", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to update")
		filterFieldsJSON := fs.String("filter-fields-json", "", "JSON array of declared filter field definitions")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" || *filterFieldsJSON == "" {
			fmt.Fprintln(stderr, "--db, --collection, and --filter-fields-json are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:     *db,
			Command:          bridge.CommandUpdateOperationalFilters,
			CollectionName:   *collectionName,
			FilterFieldsJSON: *filterFieldsJSON,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "update-operational-validation":
		fs := flag.NewFlagSet("update-operational-validation", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to update")
		validationJSON := fs.String("validation-json", "", "JSON validation contract for the collection")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		hasValidationJSON := false
		fs.Visit(func(flag *flag.Flag) {
			if flag.Name == "validation-json" {
				hasValidationJSON = true
			}
		})
		if *db == "" || *collectionName == "" || !hasValidationJSON {
			fmt.Fprintln(stderr, "--db, --collection, and --validation-json are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandUpdateOperationalValidation,
			CollectionName: *collectionName,
			ValidationJSON: *validationJSON,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "update-operational-secondary-indexes":
		fs := flag.NewFlagSet("update-operational-secondary-indexes", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to update")
		secondaryIndexesJSON := fs.String("secondary-indexes-json", "", "JSON array of declared secondary index definitions")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" || *secondaryIndexesJSON == "" {
			fmt.Fprintln(stderr, "--db, --collection, and --secondary-indexes-json are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:         *db,
			Command:              bridge.CommandUpdateOperationalIndexes,
			CollectionName:       *collectionName,
			SecondaryIndexesJSON: *secondaryIndexesJSON,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "validate-operational-history":
		fs := flag.NewFlagSet("validate-operational-history", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to validate")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandValidateOperationalHistory,
			CollectionName: *collectionName,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "rebuild-operational-secondary-indexes":
		fs := flag.NewFlagSet("rebuild-operational-secondary-indexes", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to rebuild")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandRebuildOperationalIndexes,
			CollectionName: *collectionName,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "disable-operational":
		fs := flag.NewFlagSet("disable-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to disable")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandDisableOperationalCollection,
			CollectionName: *collectionName,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "plan-operational-retention":
		fs := flag.NewFlagSet("plan-operational-retention", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionsJSON := fs.String("collections-json", "", "optional JSON array of collection names")
		nowTimestamp := fs.Int64("now", 0, "optional unix timestamp override")
		maxCollections := fs.Int("max-collections", 0, "optional maximum collections to examine")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		var collectionNames []string
		if *collectionsJSON != "" {
			if err := json.Unmarshal([]byte(*collectionsJSON), &collectionNames); err != nil {
				fmt.Fprintf(stderr, "invalid --collections-json: %v\n", err)
				return 2
			}
		}
		now := *nowTimestamp
		if now == 0 {
			now = time.Now().Unix()
		}
		request := bridge.Request{
			DatabasePath:    *db,
			Command:         bridge.CommandPlanOperationalRetention,
			CollectionNames: collectionNames,
			NowTimestamp:    now,
			MaxCollections:  *maxCollections,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "run-operational-retention":
		fs := flag.NewFlagSet("run-operational-retention", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionsJSON := fs.String("collections-json", "", "optional JSON array of collection names")
		nowTimestamp := fs.Int64("now", 0, "optional unix timestamp override")
		maxCollections := fs.Int("max-collections", 0, "optional maximum collections to examine")
		dryRun := fs.Bool("dry-run", false, "report retention actions without mutation")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		var collectionNames []string
		if *collectionsJSON != "" {
			if err := json.Unmarshal([]byte(*collectionsJSON), &collectionNames); err != nil {
				fmt.Fprintf(stderr, "invalid --collections-json: %v\n", err)
				return 2
			}
		}
		now := *nowTimestamp
		if now == 0 {
			now = time.Now().Unix()
		}
		request := bridge.Request{
			DatabasePath:    *db,
			Command:         bridge.CommandRunOperationalRetention,
			CollectionNames: collectionNames,
			NowTimestamp:    now,
			MaxCollections:  *maxCollections,
			DryRun:          *dryRun,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "compact-operational":
		fs := flag.NewFlagSet("compact-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to compact")
		dryRun := fs.Bool("dry-run", false, "report the compaction result without mutating the database")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" {
			fmt.Fprintln(stderr, "--db and --collection are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandCompactOperationalCollection,
			CollectionName: *collectionName,
			DryRun:         *dryRun,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "purge-operational":
		fs := flag.NewFlagSet("purge-operational", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "operational collection name to purge")
		beforeTimestamp := fs.Int64("before", 0, "delete mutations older than this unix timestamp")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *collectionName == "" || *beforeTimestamp == 0 {
			fmt.Fprintln(stderr, "--db, --collection, and --before are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:    *db,
			Command:         bridge.CommandPurgeOperationalCollection,
			CollectionName:  *collectionName,
			BeforeTimestamp: *beforeTimestamp,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "purge-provenance-events":
		fs := flag.NewFlagSet("purge-provenance-events", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		beforeTimestamp := fs.Int64("before-timestamp", 0, "delete provenance events older than this unix timestamp")
		preserveEventTypes := fs.String("preserve-event-types", "", "comma-separated event types to preserve")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *beforeTimestamp == 0 {
			fmt.Fprintln(stderr, "--db and --before-timestamp are required")
			return 2
		}
		var preserveTypes []string
		if *preserveEventTypes != "" {
			preserveTypes = strings.Split(*preserveEventTypes, ",")
		}
		request := bridge.Request{
			DatabasePath:       *db,
			Command:            bridge.CommandPurgeProvenanceEvents,
			BeforeTimestamp:    *beforeTimestamp,
			PreserveEventTypes: preserveTypes,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "rebuild":
		fs := flag.NewFlagSet("rebuild", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		target := fs.String("target", "all", "projection target: fts|vec|all")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandRebuildProjections,
			Target:       *target,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "rebuild-operational-current":
		fs := flag.NewFlagSet("rebuild-operational-current", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		collectionName := fs.String("collection", "", "optional operational collection name to rebuild")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		request := bridge.Request{
			DatabasePath:   *db,
			Command:        bridge.CommandRebuildOperationalCurrent,
			CollectionName: *collectionName,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "rebuild-missing":
		fs := flag.NewFlagSet("rebuild-missing", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandRebuildMissing,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "regenerate-vectors":
		fs := flag.NewFlagSet("regenerate-vectors", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		configPath := fs.String("config", "", "path to TOML or JSON vector regeneration contract")
		defaultPolicy := bridge.DefaultVectorGeneratorPolicy()
		generatorTimeoutMS := fs.Uint64("generator-timeout-ms", defaultPolicy.TimeoutMS, "wall-clock timeout for the external vector generator")
		generatorMaxStdoutBytes := fs.Int("generator-max-stdout-bytes", defaultPolicy.MaxStdoutBytes, "maximum stdout bytes allowed from the external vector generator")
		generatorMaxStderrBytes := fs.Int("generator-max-stderr-bytes", defaultPolicy.MaxStderrBytes, "maximum stderr bytes allowed from the external vector generator")
		generatorMaxInputBytes := fs.Int("generator-max-input-bytes", defaultPolicy.MaxInputBytes, "maximum JSON stdin bytes sent to the external vector generator")
		generatorMaxChunks := fs.Int("generator-max-chunks", defaultPolicy.MaxChunks, "maximum chunk count allowed in one vector regeneration run")
		var generatorAllowedRoots stringSliceFlag
		var generatorPreserveEnv stringSliceFlag
		fs.Var(&generatorAllowedRoots, "generator-allowed-root", "allowlisted root for the external vector generator executable (repeatable)")
		fs.Var(&generatorPreserveEnv, "generator-preserve-env", "environment variable to preserve for the external vector generator (repeatable)")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *configPath == "" {
			fmt.Fprintln(stderr, "--db and --config are required")
			return 2
		}
		if *generatorMaxStdoutBytes <= 0 || *generatorMaxStderrBytes <= 0 || *generatorMaxInputBytes <= 0 || *generatorMaxChunks <= 0 {
			fmt.Fprintln(stderr, "generator limits must be greater than zero")
			return 2
		}
		if err := commands.RunRegenerateVectorsWithFeedback(
			*db,
			*bridgeBinary,
			*configPath,
			&bridge.VectorGeneratorPolicy{
				TimeoutMS:                     *generatorTimeoutMS,
				MaxStdoutBytes:                *generatorMaxStdoutBytes,
				MaxStderrBytes:                *generatorMaxStderrBytes,
				MaxInputBytes:                 *generatorMaxInputBytes,
				MaxChunks:                     *generatorMaxChunks,
				RequireAbsoluteExecutable:     defaultPolicy.RequireAbsoluteExecutable,
				RejectWorldWritableExecutable: defaultPolicy.RejectWorldWritableExecutable,
				AllowedExecutableRoots:        append([]string(nil), generatorAllowedRoots...),
				PreserveEnvVars:               append([]string(nil), generatorPreserveEnv...),
			},
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "excise":
		fs := flag.NewFlagSet("excise", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary")
		sourceRef := fs.String("source-ref", "", "source reference to excise")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *sourceRef == "" {
			fmt.Fprintln(stderr, "--db and --source-ref are required")
			return 2
		}
		request := bridge.Request{
			DatabasePath: *db,
			Command:      bridge.CommandExciseSource,
			SourceRef:    *sourceRef,
		}
		if err := commands.RunBridgeCommandWithFeedback(
			bridge.Client{BinaryPath: *bridgeBinary},
			request,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "recover":
		fs := flag.NewFlagSet("recover", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to the (possibly corrupt) source sqlite database")
		dest := fs.String("dest", "", "path where the recovered database will be written (must not already exist)")
		bridgeBinary := fs.String("bridge", "", "path to admin bridge binary (optional; bootstraps fathomdb schema and enables Layer 2 checks)")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" || *dest == "" {
			fmt.Fprintln(stderr, "--db and --dest are required")
			return 2
		}
		if err := commands.RunRecoverWithFeedback(
			*db,
			*dest,
			*bridgeBinary,
			"",
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "repair":
		fs := flag.NewFlagSet("repair", flag.ContinueOnError)
		fs.SetOutput(stderr)
		db := fs.String("db", cfg.DatabasePath, "path to sqlite database")
		bridgeBinary := fs.String("bridge", cfg.BridgeBinary, "path to admin bridge binary (optional; used for follow-up checks only)")
		target := fs.String("target", commands.RepairTargetAll, "repair target: all|duplicate-active|runtime-fk|orphaned-chunks")
		dryRun := fs.Bool("dry-run", false, "report planned repairs without mutating the database")
		if err := fs.Parse(args[1:]); err != nil {
			return 2
		}
		if *db == "" {
			fmt.Fprintln(stderr, "--db is required")
			return 2
		}
		if err := commands.RunRepairWithFeedback(
			*db,
			*bridgeBinary,
			"",
			*target,
			*dryRun,
			stdout,
			newFeedbackObserver(stderr),
			bridge.FeedbackConfig{},
		); err != nil {
			fmt.Fprintln(stderr, err)
			return commandExitCode(err)
		}
		return 0
	case "version":
		if err := commands.RunVersion(stdout); err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		return 0
	default:
		fmt.Fprintln(stderr, usage())
		return 2
	}
}

type stringSliceFlag []string

func (s *stringSliceFlag) String() string {
	return strings.Join(*s, ",")
}

func (s *stringSliceFlag) Set(value string) error {
	*s = append(*s, value)
	return nil
}

func commandExitCode(err error) int {
	var coded interface{ ExitCode() int }
	if errors.As(err, &coded) {
		return coded.ExitCode()
	}
	return bridge.ExitCodeFromError(err)
}

func usage() string {
	return "usage: fathom-integrity <check|export|trace|restore-logical-id|purge-logical-id|trace-operational|read-operational|update-operational-filters|update-operational-validation|validate-operational-history|disable-operational|compact-operational|purge-operational|purge-provenance-events|rebuild|rebuild-operational-current|rebuild-missing|regenerate-vectors|excise|recover|repair|version> [flags]"
}
